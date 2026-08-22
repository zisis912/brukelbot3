use std::{cmp::Ordering, collections::HashMap, fmt, sync::Arc, time::Duration};

use crossbeam::channel::Receiver;
use enum_discriminant::discriminant;
use rand::seq::SliceRandom;
use serenity::{
    all::prelude::Mentionable,
    http::Http,
    model::id::{ChannelId, UserId},
};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use tokio::time::{self, Instant};

use crate::{database::Database, events::GameEvent};

#[derive(Clone)]
pub struct PokerPlayer {
    id: UserId,
    hand: CardHand,
    bet: i64,
    last_move: Option<PokerMove>,
    funds: i64,
    allin: bool,
}

pub struct PokerGame {
    // id of the game
    id: GameId,
    creation_date: Instant,
    turn: u8,
    last_play: Instant,
    players: Vec<PokerPlayer>,
    // ante will be removed from the balance of ALL
    // players at showdown / when they fold
    ante: i64,
    has_sent_message: bool,
    // leftover cash from players which have folded
    side_pot: i64,
    // Call = 0 means the game hasnt opened
    call: i64,
}

impl PokerGame {
    pub fn new(
        challenger: UserId,
        challenger_funds: i64,
        invitees: Vec<UserId>,
        ante: i64,
    ) -> Self {
        let mut players: Vec<PokerPlayer> = invitees
            .iter()
            .map(|id| PokerPlayer {
                id: *id,
                accepted: false,
                hand: TEMPLATE_HAND,
                bet: ante,
                last_move: None,
                funds: 0,
                allin: false,
            })
            .collect();

        let chall_ref = players.iter_mut().find(|p| p.id == challenger).unwrap();

        chall_ref.accepted = true;
        chall_ref.funds = challenger_funds;

        let mut r = rand::rng();
        // shuffle player order
        players.shuffle(&mut r);

        Self {
            challenger,
            players,
            creation_date: Instant::now(),
            ante, // state: GameState::Waiting,
            state: GameState::Waiting,
            has_sent_message: false,
            turn: 0,
            // has_opened: false,
            side_pot: 0,
            call: 0,
        }
    }

    pub fn start(
        mut self,
        db: Database,
        channel_id: ChannelId,
        http: Arc<Http>,
        event_rx: Receiver<GameEvent>,
    ) {
        let mut interval = time::interval(Duration::from_secs(1));

        tokio::spawn(async move {
            loop {
                interval.tick().await;

                // While invite is outgoing:
                if self.state == GameState::Waiting {
                    // if everybody accepts, start
                    if self.players.iter().all(|p| p.accepted) {
                        // GIVE CARDS TO EVERYBODY
                        self.deal_random_cards();

                        channel_id
                            .say(
                                &http,
                                format!(
                                    "--STARTING POKER--
                                                Turn Order: {}
                                                Ante(minimum starting bet): {} niggapoints
                                                Total money in the pot: {} 
                                                Commands: /call /raise /fold /check",
                                    self.players
                                        .iter()
                                        .map(|p| p.id.mention().to_string())
                                        .collect::<Vec<_>>()
                                        .join(" -> "),
                                    self.ante,
                                    self.pot_money()
                                ),
                            )
                            .await
                            .unwrap();
                        self.state = GameState::Started {
                            // todo: use last play for auto ff
                            last_play: Instant::now(),
                            // current_player: self.players[0].id,
                        }
                    } else
                    // if inv isnt accepted within 30s quit
                    if Instant::now().duration_since(self.creation_date)
                        > Duration::from_secs(30)
                    {
                        channel_id
                            .say(
                                &http,
                                format!(
                                    "{}'s invite took more than 30s, expired",
                                    self.challenger.mention()
                                ),
                            )
                            .await
                            .unwrap();
                        break;
                    } else {
                        while let Ok(event) = event_rx.try_recv() {
                            match event {
                                // if a single person cancels, quit
                                GameEvent::PokerCancel { invitee, inviter } => {
                                    if inviter == self.challenger
                                        && self.players.iter().any(|p| p.id == invitee)
                                    {
                                        channel_id
                                            .say(
                                                &http,
                                                format!(
                                                    "{} didnt accept poker, disbanding lobby",
                                                    invitee.mention()
                                                ),
                                            )
                                            .await
                                            .unwrap();
                                        break;
                                    }
                                }
                                // player accepts
                                GameEvent::PokerAccept {
                                    invitee,
                                    inviter,
                                    funds,
                                } => {
                                    if inviter == self.challenger
                                        && self.players.iter().any(|p| p.id == invitee)
                                    {
                                        if db.nigga_balance(invitee).await.unwrap()
                                            < (self.ante + funds).try_into().unwrap()
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
                                        self.player_accepts_inv(invitee, funds);
                                        channel_id.say(&http, "accepted invite").await.unwrap();
                                        continue;
                                    }
                                }
                                _ => continue,
                            }
                        }
                    }
                }

                // 1 player left, instawin
                if self.players.len() == 1 {
                    let winner = &self.players[0];
                    channel_id
                        .say(
                            &http,
                            format!(
                                "Winner: {}, receives {} niggapoints",
                                winner.id.mention(),
                                self.pot_money()
                            ),
                        )
                        .await
                        .unwrap();

                    // the whole pot goes to the winner
                    // todo typesafety
                    db.nigga_increment(winner.id, self.pot_money() as i64)
                        .await
                        .unwrap();
                    break;
                }

                // free round, reshuffle
                if self.everybody_checked() {
                    channel_id
                        .say(&http, "Everybody checked, new cards")
                        .await
                        .unwrap();

                    self.deal_random_cards();
                    for player in self.players.iter_mut() {
                        player.last_move = None;
                    }
                    self.has_sent_message = false;
                }

                // SHOWDOWN
                if self.everybody_called() {
                    channel_id
                        .say(&http, "Everybody called, SHOWDOWN (wip)")
                        .await
                        .unwrap();

                    let winner = &self
                        .players
                        .iter()
                        .max_by_key(|p| PokerHand::from(p.hand))
                        .unwrap();

                    channel_id
                        .say(
                            &http,
                            format!(
                                "{} IS THE WINNER!! They win {} nig total",
                                winner.id.mention(),
                                self.pot_money() - winner.bet
                            ),
                        )
                        .await
                        .unwrap();

                    for player in &self.players {
                        db.nigga_increment(player.id, -(player.bet as i64))
                            .await
                            .unwrap();
                        self.side_pot += player.bet
                    }
                    db.nigga_increment(winner.id, self.side_pot as i64)
                        .await
                        .unwrap();

                    break;
                }

                // From here on we assume self.has started

                if self.player_in_turn().allin {
                    channel_id
                        .say(
                            &http,
                            format!(
                                "{} has already gone all-in, skip betting",
                                self.player_in_turn().id.mention()
                            ),
                        )
                        .await
                        .unwrap();
                    self.advance_turn();
                    continue;
                }

                // info update
                if !self.has_sent_message {
                    channel_id
                        .say(
                            &http,
                            format!(
                                "It is {}'s turn, his funds: {}
                                        his current bet: {}
                                        Minimum call: {}
                                        His cards: {} ({:?})
                                        
                                        Total Money in the pot: {} niggapoints
                                        
                                        ",
                                self.player_in_turn().id.mention(),
                                self.player_in_turn().funds,
                                self.player_in_turn().bet,
                                self.call,
                                self.player_in_turn().hand.print_hand(),
                                PokerHand::from(self.player_in_turn().hand),
                                self.pot_money(),
                            ),
                        )
                        .await
                        .unwrap();
                    self.has_sent_message = true
                }

                // player checks
                match event_rx.try_recv() {
                    Ok(GameEvent::PokerCheck { checker }) => {
                        if checker == self.player_in_turn().id {
                            if self.call == 0 {
                                //check
                                channel_id.say(&http, "checked").await.unwrap();
                                self.play_turn(PokerMove::Check);
                            } else {
                                channel_id
                                    .say(&http, "You cant check if the betting round has opened")
                                    .await
                                    .unwrap();
                            }
                        }
                    }

                    // player folds
                    Ok(GameEvent::PokerFold { folder }) => {
                        if folder == self.player_in_turn().id {
                            //fold
                            let folder = self.player_in_turn().clone();
                            db.nigga_increment(folder.id, -(folder.bet as i64))
                                .await
                                .unwrap();
                            self.fold_current_player();
                            channel_id
                                .say(
                                    &http,
                                    format!("{} Folded, he lost {}", folder.id.clone(), folder.bet),
                                )
                                .await
                                .unwrap();
                        }
                    }

                    // player raises
                    Ok(GameEvent::PokerRaise { raiser, amount }) => {
                        if raiser == self.player_in_turn().id {
                            if amount == 0 {
                                channel_id
                                    .say(&http, format!("You cant raise by zero",))
                                    .await
                                    .unwrap();
                                continue;
                            }

                            let raiser = self.player_in_turn().clone();

                            if raiser.funds < self.call + amount {
                                channel_id
                                    .say(&http, format!("you dont have that much money to raise, current funds left: {}",raiser.funds,))
                                    .await
                                    .unwrap();
                                continue;
                            }

                            self.call += amount;

                            // special case for allin
                            if raiser.funds == self.call {
                                self.allin_current_player();

                                channel_id
                                    .say(
                                        &http,
                                        format!(
                                            "{} Raised by {} and went all-in!",
                                            raiser.id.mention(),
                                            amount
                                        ),
                                    )
                                    .await
                                    .unwrap();
                            } else {
                                self.bet_current_player(self.call);

                                channel_id
                                    .say(
                                        &http,
                                        format!("{} Raised by {}", raiser.id.mention(), amount),
                                    )
                                    .await
                                    .unwrap();
                            }
                            self.play_turn(PokerMove::Raise);
                            // self.has_opened = true;
                            continue;
                        }
                    }

                    // player calls
                    Ok(GameEvent::PokerCall { caller }) => {
                        if caller == self.player_in_turn().id {
                            if self.call == 0 {
                                channel_id
                                    .say(
                                        &http,
                                        format!("cant call if self.hasnt opened, raise first",),
                                    )
                                    .await
                                    .unwrap();
                                continue;
                            }

                            let caller = self.player_in_turn().clone();

                            if caller.funds <= self.call {
                                channel_id
                                    .say(
                                        &http,
                                        format!(
                                            "{} goes all in with {} nigpoints!",
                                            caller.id.mention(),
                                            caller.bet + caller.funds
                                        ),
                                    )
                                    .await
                                    .unwrap();

                                self.allin_current_player();
                            } else {
                                channel_id
                                    .say(
                                        &http,
                                        format!(
                                            "{} calls the {} $!",
                                            caller.id.mention(),
                                            self.call + self.ante
                                        ),
                                    )
                                    .await
                                    .unwrap();

                                self.bet_current_player(self.call - caller.bet);
                            }
                            self.play_turn(PokerMove::Call);
                        }
                    }
                    _ => continue,
                }
            }
        });
    }
    // fn player_from_id(&self, id: u64) -> &PokerPlayerState {
    //     self.players.iter().find(|p| p.id == id).unwrap()
    // }

    fn allin_current_player(&mut self) {
        self.players[self.turn as usize].bet += self.player_in_turn().funds;
        self.players[self.turn as usize].funds = 0;
        self.players[self.turn as usize].allin = true;
    }

    fn bet_current_player(&mut self, amount: i64) {
        self.players[self.turn as usize].funds -= amount;
        self.players[self.turn as usize].bet += amount;
    }

    fn pot_money(&self) -> i64 {
        self.players.iter().map(|p| p.bet).sum::<i64>() + self.side_pot
    }
    fn player_accepts_inv(&mut self, player: UserId, funds: i64) {
        if let Some(player) = self.players.iter_mut().find(|p| p.id == player) {
            player.accepted = true;
            player.funds = funds;
        };
    }
    fn advance_turn(&mut self) {
        self.turn = (self.turn + 1) % self.players.len() as u8;
    }
    fn player_in_turn(&self) -> &PokerPlayer {
        &self.players[self.turn as usize]
    }
    fn fold_current_player(&mut self) {
        let player_to_kick = (*self.player_in_turn()).clone();
        self.players.retain(|p| p.id != player_to_kick.id);
        self.turn = self.turn % self.players.len() as u8;
        self.has_sent_message = false;
        self.side_pot += player_to_kick.bet;
    }
    fn play_turn(&mut self, mov: PokerMove) {
        self.players[self.turn as usize].last_move = Some(mov);
        // match move {
        //     PokerMove::Check =>
        // }
        self.advance_turn();
        self.has_sent_message = false;
    }

    fn everybody_checked(&self) -> bool {
        self.players
            .iter()
            .all(|p| p.last_move == Some(PokerMove::Check) || p.allin)
    }

    fn everybody_called(&self) -> bool {
        self.players
            .iter()
            .all(|p| p.last_move == Some(PokerMove::Call))
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
pub enum PokerMove {
    Call,
    Raise,
    Fold,
    Check,
}

#[derive(Clone, Copy)]
struct CardHand([NormalCard; 5]);

const TEMPLATE_HAND: CardHand = CardHand(
    [NormalCard {
        rank: CardRank::Ace,
        suit: CardSuit::Diamonds,
    }; 5],
);

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
        let four_of_a_kind = ranks.iter().find(|(_, v)| **v == 4);

        // card rank that exists EXACTLY 3 times
        let three_of_a_kind = ranks.iter().find(|(_, v)| **v == 3);

        // card ranks that exist less than 3 times (unique, sorted highest-> lowest)
        let mut not_three_of_a_kind: Vec<CardRank> = ranks
            .iter()
            .filter_map(|(k, v)| (*v != 3).then_some(*k))
            .collect();
        not_three_of_a_kind.sort_by(|c1, c2| c1.cmp(c2).reverse());

        // card ranks that exist EXACTLY 2 times ( sorted highest->lowest)
        let mut two_of_a_kind: Vec<CardRank> = ranks
            .iter()
            .filter_map(|(k, v)| (*v == 2).then_some(*k))
            .collect();
        two_of_a_kind.sort_by(|c1, c2| c1.cmp(c2).reverse());

        // card ranks which are NOT 2 of a kind (sorted highest-> lowest)
        let mut not_two_of_a_kind: Vec<CardRank> = ranks
            .iter()
            .filter_map(|(k, v)| (*v != 2).then_some(*k))
            .collect();
        not_two_of_a_kind.sort_by_key(|c| *c as u8);
        not_two_of_a_kind.reverse();

        // one of a kind
        let one_of_a_kind = ranks.iter().find(|(_, v)| **v == 1);

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

// impl PokerHand {
// fn print_variant(&self) -> &str {
//     match self {
//         _ => "",
//     }
//     // TODO TOMORROW
// }
// }

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
