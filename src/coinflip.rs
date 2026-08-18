use std::time::Duration;

use rand::{random, random_bool};
use serenity::all::{Context, Message};
use tokio::time::{self, Instant};

use crate::{
    CommandError::{self, InvalidArgCount},
    Handler, nigga_increment,
};

pub struct CoinflipPlayerState {
    pub id: u64,
    pub accepted: bool,
}

struct CoinflipGame {
    p1: CoinflipPlayerState,
    p2: CoinflipPlayerState,
    invite_time: Instant,
    bid: u64,
    // state: GameState
}

#[derive(Clone, Debug)]
pub enum GameEvent {
    CoinflipAcceptEvent {
        p2: u64,
        p1: u64,
    },
    CoinflipCancelEvent {
        p1: u64,
        p2: u64,
    },
    PokerAcceptEvent {
        invitee: u64,
        inviter: u64,
        funds: u64,
    },
    PokerCancelEvent {
        invitee: u64,
        inviter: u64,
    },
    PokerCall {
        caller: u64,
    },
    PokerRaise {
        raiser: u64,
        amount: u64,
    },
    PokerFold {
        folder: u64,
    },
    PokerCheck {
        checker: u64,
    },
}

impl Handler {
    // coinflip command
    pub async fn coinflip(
        &self,
        msg: &Message,
        ctx: &Context,
        args: Vec<&str>,
    ) -> Result<(), CommandError> {
        match args[0] {
            "send" => {
                if args.len() != 3 {
                    return Err(InvalidArgCount);
                }

                // WHY IS THIS NECESSARY
                let channel_id = msg.channel_id;
                // let chall_name = msg.author.name.clone();

                let chall_id = msg.author.id.get();
                let opp_id = self.resolve_user_mention(args[1])?;

                let chall_bal = self.nigga_balance(chall_id).await?;
                let opp_bal = self.nigga_balance(opp_id).await?;

                let bid: u64 = args[2].parse()?;

                let i64bid: i64 = bid.try_into()?;
                if i64bid > chall_bal || i64bid > opp_bal {
                    msg.channel_id
                        .say(&ctx.http, "one of u doesnt have enough money for the bid")
                        .await?;
                    return Ok(());
                }

                let http = ctx.http.clone();
                let mut event_listener = self.channel_send.subscribe();

                let pool = self.pool.clone();

                channel_id
                    .say(
                        &http,
                        format!(
                            "sending coinflip inv to <@{}> with bid {} niggapoints, they got 30 seconds to accept", 
                            &opp_id, &bid
                        ),
                    )
                    .await?;

                tokio::spawn(async move {
                    let mut interval = time::interval(Duration::from_secs(1));

                    let mut game = CoinflipGame {
                        p1: CoinflipPlayerState {
                            id: chall_id,
                            accepted: true,
                        },
                        p2: CoinflipPlayerState {
                            id: opp_id,
                            accepted: false,
                        },
                        invite_time: Instant::now(),
                        bid, // state: GameState::Waiting,
                    };

                    // move msg

                    loop {
                        interval.tick().await;

                        // if invite isnt accepted within 30s quit
                        if Instant::now().duration_since(game.invite_time) > Duration::from_secs(30)
                        {
                            channel_id
                                .say(
                                    http,
                                    format!(
                                        "coinflip inv by <@{}> against <@{}> expired",
                                        chall_id, opp_id
                                    ),
                                )
                                .await
                                .unwrap();
                            break;
                        }

                        // player 2 accepts
                        if let Ok(GameEvent::CoinflipAcceptEvent { p2, p1 }) =
                            event_listener.try_recv()
                            && game.p1.id == p1
                            && game.p2.id == p2
                        {
                            game.p2.accepted = true;
                        }

                        // player 1 cancels
                        if let Ok(GameEvent::CoinflipCancelEvent { p1, p2 }) =
                            event_listener.try_recv()
                            && game.p1.id == p1
                            && game.p2.id == p2
                        {
                            game.p2.accepted = false;
                        }

                        // if both cancel delete game

                        if !game.p1.accepted && !game.p2.accepted {
                            break;
                        }

                        // if both agree commence
                        if game.p1.accepted && game.p2.accepted {
                            channel_id
                                .say(
                                    &http,
                                    format!(
                                        "COINFLIP BATTLE:\n <@{}> against <@{}> STARTS",
                                        chall_id, opp_id
                                    ),
                                )
                                .await
                                .unwrap();

                            let p1_wins = random_bool(0.5);
                            let (winner, loser) = if p1_wins {
                                (game.p1.id, game.p2.id)
                            } else {
                                (game.p2.id, game.p1.id)
                            };

                            channel_id
                                .say(&http, format!("<@{}>  WINS {} nigpoints", winner, &bid))
                                .await
                                .unwrap();

                            nigga_increment(&pool, winner, i64bid).await.unwrap();
                            nigga_increment(&pool, loser, -i64bid).await.unwrap();

                            break;
                        }
                    }
                });
            }
            "accept" => {
                if args.len() != 2 {
                    return Err(InvalidArgCount);
                }

                let accepter_id = msg.author.id.get();
                let chall_id = self.resolve_user_mention(args[1])?;
                self.channel_send.send(GameEvent::CoinflipAcceptEvent {
                    p2: accepter_id,
                    p1: chall_id,
                })?;
            }
            "cancel" => {
                if args.len() != 2 {
                    return Err(InvalidArgCount);
                }

                let chall_id = msg.author.id.get();
                let opp_id = self.resolve_user_mention(args[1])?;
                self.channel_send.send(GameEvent::CoinflipCancelEvent {
                    p1: chall_id,
                    p2: opp_id,
                })?;
            }
            _ => {}
        };
        Ok(())
    }
}
