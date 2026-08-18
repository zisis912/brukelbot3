use std::{cmp::Ordering, collections::HashMap, fmt, time::Duration};

use enum_discriminant::discriminant;
use rand::seq::SliceRandom;
use serenity::{
    all::{Context, Message},
    http,
};
use tokio::time::{self, Instant};

use crate::{
    CommandError::{self, InvalidArgCount},
    Handler,
    coinflip::GameEvent,
    nigga_balance, nigga_increment,
};

use strum::IntoEnumIterator;
use strum_macros::EnumIter;

#[derive(PartialEq)]
pub enum GameState {
    Started {
        last_play: Instant,
        // current_player: u64,
    },
    Waiting,
}

#[derive(Clone, Debug, Copy, EnumIter, PartialEq, Eq, Hash)]
#[discriminant(u8)]
// #[repr(u8)]
pub enum CardRank {
    Ace = 1,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
}

#[derive(Clone, Copy, EnumIter, PartialEq, Eq)]
enum CardSuit {
    Diamonds,
    Clubs,
    Hearts,
    Spades,
}

#[derive(Clone, Copy)]
enum SpecialCard {}

#[derive(Clone, Copy)]
enum PokerCard {
    Special(SpecialCard),
    Normal(NormalCard),
}

#[derive(Clone, Copy, PartialEq)]

struct NormalCard {
    rank: CardRank,
    suit: CardSuit,
}

#[derive(PartialEq, Clone)]
enum PokerMove {
    Call,
    Raise,
    Fold,
    Check,
}

#[derive(Clone, Copy)]
struct CardHand([NormalCard; 5]);

#[derive(Clone)]
struct PokerPlayerState {
    pub id: u64,
    pub accepted: bool,
    hand: CardHand,
    bet: u64,
    last_move: PokerMove,
    funds: u64,
    allin: bool,
}

struct PokerGame {
    players: Vec<PokerPlayerState>,
    most_recent_join: Instant,
    ante: u64,
    state: GameState,
    has_sent_message: bool,
    turn: u8,
    // has_opened: bool,
    // anonymous cash from players which have left
    pot: u64,
    call: u64,
}

#[discriminant(u8)]
#[derive(PartialEq, Eq, Debug)]
pub enum PokerHand {
    // consecutive + same suit
    StraightFlush(CardRank) = 0,
    // 4
    FourOfAKind {
        quad: CardRank,
        single: CardRank,
    },
    // 3 + 2
    FullHouse {
        triad: CardRank,
        pair: CardRank,
    },
    // same suit
    Flush([CardRank; 5]),
    // consecutive
    Straight(CardRank),
    // 3
    ThreeOfAKind {
        triad: CardRank,
        singles: [CardRank; 2],
    },
    // 2 + 2
    TwoPair {
        pair1: CardRank,
        pair2: CardRank,
        single: CardRank,
    },
    // 2
    OnePair {
        pair: CardRank,
        singles: [CardRank; 3],
    },
    HighCard([CardRank; 5]),
}

impl From<CardHand> for PokerHand {
    fn from(hand: CardHand) -> Self {
        let mut hand = hand.0;

        // Sort hand from highest to lowest
        hand.sort_by(|c1, c2| c1.rank.cmp(&c2.rank).reverse());

        let mut ranks: HashMap<CardRank, u8> = HashMap::new();
        for card in &hand {
            *ranks.entry(card.rank).or_insert(0) += 1;
        }

        let max_rank = hand[0].rank;

        // if every card's suit is equal to its previous, flush
        let flush = hand.windows(2).all(|w| w[0].suit == w[1].suit);

        // if every card's rank is 1 less than its previous, straight
        let straight = hand
            .windows(2)
            .all(|w| w[0].rank as u8 - w[1].rank as u8 == 1);

        // card rank that exists EXACTLY 4 times
        let four_of_a_kind = ranks.iter().find(|(k, v)| **v == 4);

        // card rank that exists EXACTLY 3 times
        let three_of_a_kind = ranks.iter().find(|(k, v)| **v == 3);

        // card ranks that exist less than 3 times (unique, sorted)
        let mut not_three_of_a_kind: Vec<CardRank> = ranks
            .iter()
            .filter(|(k, v)| **v != 3)
            .map(|(k, v)| *k)
            .collect();
        not_three_of_a_kind.sort_by(|c1, c2| c1.cmp(c2).reverse());

        // card ranks that exist EXACTLY 2 times ( sorted highest->lowest)
        let mut two_of_a_kind: Vec<CardRank> = ranks
            .iter()
            .filter(|(k, v)| **v == 2)
            .map(|(k, v)| *k)
            .collect();
        two_of_a_kind.sort_by(|c1, c2| c1.cmp(c2).reverse());

        let mut not_two_of_a_kind: Vec<CardRank> = ranks
            .iter()
            .filter(|(k, v)| **v != 2)
            .map(|(k, v)| *k)
            .collect();
        not_two_of_a_kind.sort_by_key(|c| *c as u8);
        not_two_of_a_kind.reverse();
        let one_of_a_kind = ranks.iter().find(|(k, v)| **v == 1);

        if flush && straight {
            return PokerHand::StraightFlush(max_rank);
        }

        if let Some((rank, _)) = four_of_a_kind
            && let Some((rank2, _)) = one_of_a_kind
        {
            return PokerHand::FourOfAKind {
                quad: *rank,
                single: *rank2,
            };
        }

        if let Some((rank, _)) = three_of_a_kind
            && let Some(rank2) = two_of_a_kind.iter().nth(0)
        {
            return PokerHand::FullHouse {
                triad: *rank,
                pair: *rank2,
            };
        }

        if flush {
            return PokerHand::Flush(hand.map(|c| c.rank));
        }

        if straight {
            return PokerHand::Straight(max_rank);
        }

        if let Some((rank1, _)) = three_of_a_kind {
            return PokerHand::ThreeOfAKind {
                triad: *rank1,
                singles: not_three_of_a_kind.try_into().unwrap(),
            };
        }

        if two_of_a_kind.len() == 2 && not_two_of_a_kind.len() == 1 {
            return PokerHand::TwoPair {
                pair1: two_of_a_kind[0],
                pair2: two_of_a_kind[1],
                single: not_two_of_a_kind[0],
            };
        }

        if two_of_a_kind.len() == 1 && not_two_of_a_kind.len() == 3 {
            return PokerHand::OnePair {
                pair: two_of_a_kind[0],
                singles: not_two_of_a_kind.try_into().unwrap(),
            };
        }

        PokerHand::HighCard(hand.map(|c| c.rank))
    }
}

impl PartialOrd for CardRank {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CardRank {
    fn cmp(&self, other: &Self) -> Ordering {
        self.discriminant().cmp(&other.discriminant())
    }
}

impl Ord for PokerHand {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // if equal return
        if self == other {
            return Ordering::Equal;
        }

        // worse discriminant = better hand so we reverse
        let hand_check = self.discriminant().cmp(&other.discriminant()).reverse();

        // if one hand is clearly better than the other return
        if hand_check != Ordering::Equal {
            return hand_check;
        }

        // from here on we are in the same enum variant / hand type, and they cant be equal

        if let PokerHand::StraightFlush(rank1) = self
            && let PokerHand::StraightFlush(rank2) = other
        {
            return rank1.cmp(&rank2);
        }

        if let PokerHand::FourOfAKind {
            quad: quad_rank1,
            single: single_rank1,
        } = self
            && let PokerHand::FourOfAKind {
                quad: four_rank2,
                single: single_rank2,
            } = other
        {
            let quad_cmp = quad_rank1.cmp(&four_rank2);
            if quad_cmp != Ordering::Equal {
                return quad_cmp;
            }
            return single_rank1.cmp(&single_rank2);
        }

        if let PokerHand::FullHouse {
            triad: triad_rank1,
            pair: pair_rank1,
        } = self
            && let PokerHand::FullHouse {
                triad: triad_rank2,
                pair: pair_rank2,
            } = other
        {
            let triad_cmp = triad_rank1.cmp(&triad_rank2);
            if triad_cmp != Ordering::Equal {
                return triad_cmp;
            }
            return pair_rank1.cmp(&pair_rank2);
        }

        if let PokerHand::Flush(cards1) = self
            && let PokerHand::Flush(cards2) = other
        {
            // compare each card of the 2 hands one by one starting from highest in each
            for n in 0..5 {
                let cmp = cards1[n].cmp(&cards2[n]);
                if cmp != Ordering::Equal {
                    return cmp;
                }
            }
        }

        if let PokerHand::Straight(rank1) = self
            && let PokerHand::Straight(rank2) = other
        {
            return rank1.cmp(&rank2);
        }

        if let PokerHand::ThreeOfAKind {
            triad: triad_rank1,
            singles: singles1,
        } = self
            && let PokerHand::ThreeOfAKind {
                triad: triad_rank2,
                singles: singles2,
            } = other
        {
            let triad_cmp = triad_rank1.cmp(&triad_rank2);
            if triad_cmp != Ordering::Equal {
                return triad_cmp;
            }
            for n in 0..2 {
                let cmp = singles1[n].cmp(&singles2[n]);
                if cmp != Ordering::Equal {
                    return cmp;
                }
            }
        }

        if let PokerHand::TwoPair {
            pair1: pair1_rank1,
            pair2: pair2_rank1,
            single: single_rank1,
        } = self
            && let PokerHand::TwoPair {
                pair1: pair1_rank2,
                pair2: pair2_rank2,
                single: single_rank2,
            } = other
        {
            let pair1_cmp = pair1_rank1.cmp(pair1_rank2);
            if pair1_cmp != Ordering::Equal {
                return pair1_cmp;
            }

            let pair2_cmp = pair2_rank1.cmp(pair2_rank2);
            if pair2_cmp != Ordering::Equal {
                return pair2_cmp;
            }
            return single_rank1.cmp(single_rank2);
        }

        if let PokerHand::OnePair {
            pair: pair_rank1,
            singles: cards1,
        } = self
            && let PokerHand::OnePair {
                pair: pair_rank2,
                singles: cards2,
            } = other
        {
            let pair_cmp = pair_rank1.cmp(pair_rank2);
            if pair_cmp != Ordering::Equal {
                return pair_cmp;
            }

            for n in 0..3 {
                let cmp = cards1[n].cmp(&cards2[n]);
                if cmp != Ordering::Equal {
                    return cmp;
                }
            }
        }
        if let PokerHand::HighCard(cards1) = self
            && let PokerHand::HighCard(cards2) = other
        {
            for n in 0..5 {
                let cmp = cards1[n].cmp(&cards2[n]);
                if cmp != Ordering::Equal {
                    return cmp;
                }
            }
        }

        // if the two variants are not equal but theyre also never equal thats impossible
        unreachable!()
    }
}

impl PokerHand {
    fn print_variant(&self) -> &str {
        match self {
            _ => "",
        }
        // TODO TOMORROW
    }
}

impl PartialOrd for PokerHand {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl CardHand {
    fn print_hand(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4]
        )
    }
}

impl fmt::Display for PokerCard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normal(card) => write!(f, "{}", card),
            Self::Special(card) => write!(f, "{}", "todo later"),
        }
    }
}

impl fmt::Display for NormalCard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.rank, self.suit)
    }
}

impl fmt::Display for CardRank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                CardRank::Ace => "A",
                CardRank::Two => "2",
                CardRank::Three => "3",
                CardRank::Four => "4",
                CardRank::Five => "5",
                CardRank::Six => "6",
                CardRank::Seven => "7",
                CardRank::Eight => "8",
                CardRank::Nine => "9",
                CardRank::Ten => "10",
                CardRank::Jack => "J",
                CardRank::Queen => "Q",
                CardRank::King => "K",
            }
        )
    }
}

impl fmt::Display for CardSuit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                CardSuit::Clubs => "♣",
                CardSuit::Diamonds => "♦",
                CardSuit::Hearts => "♥",
                CardSuit::Spades => "♠",
            }
        )
    }
}

impl PokerGame {
    // fn player_from_id(&self, id: u64) -> &PokerPlayerState {
    //     self.players.iter().find(|p| p.id == id).unwrap()
    // }

    fn allin_current_player(&mut self) {
        self.players[self.turn as usize].bet += self.player_in_turn().funds;
        self.players[self.turn as usize].funds = 0;
        self.players[self.turn as usize].allin = true;
    }

    fn bet_current_player(&mut self, amount: u64) {
        self.players[self.turn as usize].funds -= amount;
        self.players[self.turn as usize].bet += amount;
    }

    fn pot_money(&self) -> u64 {
        self.players.iter().map(|p| p.bet).sum::<u64>() + self.pot
    }
    fn player_accepts_inv(&mut self, player: u64, funds: u64) {
        if let Some(player) = self.players.iter_mut().find(|p| p.id == player) {
            player.accepted = true;
            player.funds = funds;
        };
    }
    fn advance_turn(&mut self) {
        self.turn = (self.turn + 1) % self.players.len() as u8;
    }
    fn player_in_turn(&self) -> &PokerPlayerState {
        &self.players[self.turn as usize]
    }
    fn fold_current_player(&mut self) {
        let player_to_kick = (*self.player_in_turn()).clone();
        self.players.retain(|p| p.id != player_to_kick.id);
        self.turn = self.turn % self.players.len() as u8;
        self.has_sent_message = false;
        self.pot += player_to_kick.bet;
    }
    fn play_turn(&mut self, mov: PokerMove) {
        self.players[self.turn as usize].last_move = mov;
        // match move {
        //     PokerMove::Check =>
        // }
        self.advance_turn();
        self.has_sent_message = false;
    }

    fn everybody_checked(&self) -> bool {
        self.players
            .iter()
            .all(|p| p.last_move == PokerMove::Check || p.allin)
    }

    fn everybody_called(&self) -> bool {
        self.players.iter().all(|p| p.last_move == PokerMove::Call)
    }

    fn deal_random_cards(&mut self) {
        // 52 cards deck x2 -> 104 cards
        let mut full_deck: Vec<NormalCard> = Vec::new();
        for i in CardSuit::iter() {
            for j in CardRank::iter() {
                full_deck.push(NormalCard { rank: j, suit: i });
            }
        }
        full_deck.extend(full_deck.clone());

        {
            // shuffle deck
            let mut r = rand::rng();
            full_deck.shuffle(&mut r);
        }

        // deal cards
        for (idx, player) in &mut self.players.iter_mut().enumerate() {
            player.hand = CardHand([
                full_deck[0 + idx * 5],
                full_deck[1 + idx * 5],
                full_deck[2 + idx * 5],
                full_deck[3 + idx * 5],
                full_deck[4 + idx * 5],
            ])
        }
    }
}

const TEMPLATE_HAND: CardHand = CardHand(
    [NormalCard {
        rank: CardRank::Ace,
        suit: CardSuit::Diamonds,
    }; 5],
);

impl Handler {
    // coinflip command
    pub async fn poker(
        &self,
        msg: &Message,
        ctx: &Context,
        args: Vec<&str>,
    ) -> Result<(), CommandError> {
        match args[0] {
            "send" => {
                // max 6 players
                if args.len() > 7 {
                    msg.channel_id
                        .say(&ctx.http, "cant have that many players")
                        .await?;
                }

                // min 2 players
                if args.len() < 3 {
                    msg.channel_id
                        .say(&ctx.http, "you need atleast 2 ppl")
                        .await?;

                    return Ok(());
                }

                let channel_id = msg.channel_id;
                let http = ctx.http.clone();

                // make sure theres no sign overflow
                let i64ante: i64 = args[1].parse::<u64>()?.try_into()?;
                let ante: u64 = i64ante as u64;

                // make sure theres no sign overflow
                let i64funds: i64 = args[2].parse::<u64>()?.try_into()?;
                let challenger_funds: u64 = i64funds as u64;

                let challenger = msg.author.id.get();

                let mut event_listener = self.channel_send.subscribe();

                let pool = self.pool.clone();

                // max 5 players
                let mut invitees: Vec<u64> = args[3..]
                    .iter()
                    .map(|mention| self.resolve_user_mention(*mention))
                    .collect::<Result<Vec<u64>, _>>()?;

                // add the challenger himself
                invitees.push(challenger);

                // make sure everybody can pay ante
                for &userid in &invitees {
                    if self.nigga_balance(userid).await.unwrap() < i64ante {
                        msg.channel_id
                            .say(&ctx.http, "error: one of the players cant pay the ante")
                            .await?;
                        return Ok(());
                    }
                }

                let mut players: Vec<PokerPlayerState> = invitees
                    .iter()
                    .map(|id| PokerPlayerState {
                        id: *id,
                        accepted: false,
                        hand: TEMPLATE_HAND,
                        bet: ante,
                        last_move: PokerMove::Raise,
                        funds: 0,
                        allin: false,
                    })
                    .collect();

                let chall = players.iter_mut().find(|p| p.id == challenger).unwrap();

                chall.accepted = true;
                chall.funds = challenger_funds;

                {
                    let mut r = rand::rng();
                    // shuffle player order
                    players.shuffle(&mut r);
                }

                msg.channel_id
                    .say(
                        &ctx.http,
                        format!("poker game invite with {} ante sent, waiting for accept! (expires in 30s)",ante),
                    )
                    .await?;

                tokio::spawn(async move {
                    let mut interval = time::interval(Duration::from_secs(1));

                    let mut game = PokerGame {
                        players,
                        most_recent_join: Instant::now(),
                        ante, // state: GameState::Waiting,
                        state: GameState::Waiting,
                        has_sent_message: false,
                        turn: 0,
                        // has_opened: false,
                        pot: 0,
                        call: 0,
                    };

                    loop {
                        interval.tick().await;

                        // While invite is outgoing:
                        if game.state == GameState::Waiting {
                            // if everybody accepts, start
                            if game.players.iter().all(|p| p.accepted) {
                                // GIVE CARDS TO EVERYBODY
                                game.deal_random_cards();

                                channel_id
                                    .say(
                                        &http,
                                        format!(
                                            "--STARTING POKER--
                                                Turn Order: {}
                                                Ante: {} niggapoints
                                                Total money in the pot: {} 
                                                Commands: /call /raise /fold /check",
                                            game.players
                                                .iter()
                                                .map(|p| format!("<@{}>", p.id))
                                                .collect::<Vec<_>>()
                                                .join(" -> "),
                                            game.ante,
                                            game.pot_money()
                                        ),
                                    )
                                    .await
                                    .unwrap();
                                game.state = GameState::Started {
                                    // todo: use last play for auto ff
                                    last_play: Instant::now(),
                                    // current_player: game.players[0].id,
                                }
                            } else
                            // if inv isnt accepted within 30s quit
                            if Instant::now().duration_since(game.most_recent_join)
                                > Duration::from_secs(30)
                            {
                                channel_id
                                    .say(
                                        &http,
                                        format!(
                                            "<@{}>'s invite took more than 30s, expired",
                                            challenger
                                        ),
                                    )
                                    .await
                                    .unwrap();
                                break;
                            } else {
                                match event_listener.try_recv() {
                                    // if a single person cancels, quit
                                    Ok(GameEvent::PokerCancelEvent { invitee, inviter }) => {
                                        if inviter == challenger
                                            && game.players.iter().any(|p| p.id == invitee)
                                        {
                                            channel_id
                                    .say(
                                        &http,
                                        format!(
                                            "<@{}> didnt accept poker, disbanding lobby",
                                            invitee
                                        ),
                                    )
                                    .await
                                    .unwrap();
                                            break;
                                        }
                                    }
                                    // player accepts
                                    Ok(GameEvent::PokerAcceptEvent {
                                        invitee,
                                        inviter,
                                        funds,
                                    }) => {
                                        if inviter == challenger
                                            && game.players.iter().any(|p| p.id == invitee)
                                        {
                                            // println!("pokeraccept");
                                            if nigga_balance(&pool, invitee).await.unwrap()
                                                < (game.ante + funds).try_into().unwrap()
                                            {
                                                channel_id
                                                    .say(
                                                        &http,
                                                        "you dont have enough to pay ante+funds",
                                                    )
                                                    .await
                                                    .unwrap();
                                                continue;
                                            }
                                            game.player_accepts_inv(invitee, funds);
                                            channel_id.say(&http, "accepted invite").await.unwrap();
                                            continue;
                                        }
                                    }
                                    _ => continue,
                                }
                            }
                        }

                        // 1 player left, instawin
                        if game.players.len() == 1 {
                            let winner = &game.players[0];
                            channel_id
                                .say(
                                    &http,
                                    format!(
                                        "Winner: <@{}>, receives {}",
                                        winner.id,
                                        game.pot_money()
                                    ),
                                )
                                .await
                                .unwrap();

                            // the whole pot goes to the winner
                            // todo typesafety
                            nigga_increment(&pool, winner.id, game.pot_money() as i64)
                                .await
                                .unwrap();
                            break;
                        }

                        // free round, reshuffle
                        if game.everybody_checked() {
                            channel_id
                                .say(&http, "Everybody checked, new cards")
                                .await
                                .unwrap();

                            game.deal_random_cards();
                            for player in game.players.iter_mut() {
                                player.last_move = PokerMove::Raise;
                            }
                            game.has_sent_message = false;
                        }

                        // SHOWDOWN
                        if game.everybody_called() {
                            channel_id
                                .say(&http, "Everybody called, SHOWDOWN (wip)")
                                .await
                                .unwrap();

                            let winner = &game
                                .players
                                .iter()
                                .max_by_key(|p| PokerHand::from(p.hand))
                                .unwrap();

                            channel_id
                                .say(
                                    &http,
                                    format!(
                                        "<@{}> IS THE WINNER!! They win {} nig total",
                                        winner.id,
                                        game.pot_money() - winner.bet
                                    ),
                                )
                                .await
                                .unwrap();

                            for player in &game.players {
                                nigga_increment(&pool, player.id, -(player.bet as i64))
                                    .await
                                    .unwrap();
                                game.pot += player.bet
                            }
                            nigga_increment(&pool, winner.id, game.pot as i64)
                                .await
                                .unwrap();

                            break;
                        }

                        // From here on we assume game has started

                        if game.player_in_turn().allin {
                            channel_id
                                .say(
                                    &http,
                                    format!(
                                        "<@{}> has already gone all-in, skip betting",
                                        game.player_in_turn().id
                                    ),
                                )
                                .await
                                .unwrap();
                            game.advance_turn();
                            continue;
                        }

                        // info update
                        if !game.has_sent_message {
                            channel_id
                                .say(
                                    &http,
                                    format!(
                                        "It is <@{}>'s turn, his funds: {}
                                        his current bet: {}
                                        Minimum call: {}
                                        His cards: {} ({:?})
                                        
                                        Total Money in the pot: {} niggapoints
                                        
                                        ",
                                        game.player_in_turn().id,
                                        game.player_in_turn().funds,
                                        game.player_in_turn().bet,
                                        game.call,
                                        game.player_in_turn().hand.print_hand(),
                                        PokerHand::from(game.player_in_turn().hand),
                                        game.pot_money(),
                                    ),
                                )
                                .await
                                .unwrap();
                            game.has_sent_message = true
                        }

                        // player checks
                        match event_listener.try_recv() {
                            Ok(GameEvent::PokerCheck { checker }) => {
                                if checker == game.player_in_turn().id {
                                    if game.call == 0 {
                                        //check
                                        channel_id.say(&http, "checked").await.unwrap();
                                        game.play_turn(PokerMove::Check);
                                    } else {
                                        channel_id
                                            .say(
                                                &http,
                                                "You cant check if the betting round has opened",
                                            )
                                            .await
                                            .unwrap();
                                    }
                                }
                            }

                            // player folds
                            Ok(GameEvent::PokerFold { folder }) => {
                                if folder == game.player_in_turn().id {
                                    //fold
                                    let folder = game.player_in_turn().clone();
                                    nigga_increment(&pool, folder.id, -(folder.bet as i64))
                                        .await
                                        .unwrap();
                                    game.fold_current_player();
                                    channel_id
                                        .say(
                                            &http,
                                            format!(
                                                "<@{}> Folded, he lost {}",
                                                folder.id, folder.bet
                                            ),
                                        )
                                        .await
                                        .unwrap();
                                }
                            }

                            // player raises
                            Ok(GameEvent::PokerRaise { raiser, amount }) => {
                                if raiser == game.player_in_turn().id {
                                    if amount == 0 {
                                        channel_id
                                            .say(&http, format!("You cant raise by zero",))
                                            .await
                                            .unwrap();
                                        continue;
                                    }

                                    let raiser = game.player_in_turn().clone();

                                    if raiser.funds < game.call + amount {
                                        channel_id
                                    .say(&http, format!("you dont have that much money to raise, current funds left: {}",raiser.funds,))
                                    .await
                                    .unwrap();
                                        continue;
                                    }

                                    game.call += amount;

                                    // special case for allin
                                    if raiser.funds == game.call {
                                        game.allin_current_player();

                                        channel_id
                                            .say(
                                                &http,
                                                format!(
                                                    "<@{}> Raised by {} and went all-in!",
                                                    raiser.id, amount
                                                ),
                                            )
                                            .await
                                            .unwrap();
                                    } else {
                                        game.bet_current_player(game.call);

                                        channel_id
                                            .say(
                                                &http,
                                                format!("<@{}> Raised by {}", raiser.id, amount),
                                            )
                                            .await
                                            .unwrap();
                                    }
                                    game.play_turn(PokerMove::Raise);
                                    // game.has_opened = true;
                                    continue;
                                }
                            }

                            // player calls
                            Ok(GameEvent::PokerCall { caller }) => {
                                if caller == game.player_in_turn().id {
                                    if game.call == 0 {
                                        channel_id
                                            .say(
                                                &http,
                                                format!(
                                                    "cant call if game hasnt opened, raise first",
                                                ),
                                            )
                                            .await
                                            .unwrap();
                                        continue;
                                    }

                                    let caller = game.player_in_turn().clone();

                                    if caller.funds <= game.call {
                                        channel_id
                                            .say(
                                                &http,
                                                format!(
                                                    "<@{}> goes all in with {} nigpoints!",
                                                    caller.id,
                                                    caller.bet + caller.funds
                                                ),
                                            )
                                            .await
                                            .unwrap();

                                        game.allin_current_player();
                                    } else {
                                        channel_id
                                            .say(
                                                &http,
                                                format!(
                                                    "<@{}> calls the {} $!",
                                                    caller.id,
                                                    game.call + game.ante
                                                ),
                                            )
                                            .await
                                            .unwrap();

                                        game.bet_current_player(game.call - caller.bet);
                                    }
                                    game.play_turn(PokerMove::Call);
                                }
                            }
                            _ => continue,
                        }
                    }
                });
            }
            "accept" => {
                if args.len() != 3 {
                    return Err(InvalidArgCount);
                }

                let accepter_id = msg.author.id.get();
                let chall_id = self.resolve_user_mention(args[1])?;
                let funds: u64 = args[2].parse()?;

                self.channel_send.send(GameEvent::PokerAcceptEvent {
                    invitee: accepter_id,
                    inviter: chall_id,
                    funds,
                })?;
            }
            "cancel" => {
                if args.len() != 2 {
                    return Err(InvalidArgCount);
                }

                let invitee = msg.author.id.get();
                let inviter = self.resolve_user_mention(args[1])?;
                self.channel_send
                    .send(GameEvent::PokerCancelEvent { invitee, inviter })?;
            }
            "call" => {
                if args.len() != 1 {
                    return Err(InvalidArgCount);
                }

                let caller = msg.author.id.get();
                self.channel_send.send(GameEvent::PokerCall { caller })?;
            }
            "raise" => {
                if args.len() != 2 {
                    return Err(InvalidArgCount);
                }

                let raiser = msg.author.id.get();
                let amount: u64 = args[1].parse()?;

                self.channel_send
                    .send(GameEvent::PokerRaise { raiser, amount })?;
            }
            "fold" => {
                if args.len() != 1 {
                    return Err(InvalidArgCount);
                }

                let folder = msg.author.id.get();
                self.channel_send.send(GameEvent::PokerFold { folder })?;
            }
            "check" => {
                if args.len() != 1 {
                    return Err(InvalidArgCount);
                }

                let checker = msg.author.id.get();
                self.channel_send.send(GameEvent::PokerCheck { checker })?;
            }
            _ => {}
        }
        Ok(())
    }
}
