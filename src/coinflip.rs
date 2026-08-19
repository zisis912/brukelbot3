use std::time::Duration;

use rand::random_bool;
use tokio::time::{self, Instant};

use crate::serenity;
use crate::{Context, Error};

// use crate::{
//     CommandError::{self, InvalidArgCount},
//     Handler, nigga_increment,
// };

pub struct CoinflipPlayerState {
    pub id: u64,
    pub accepted: bool,
}

struct CoinflipGame {
    inviter: CoinflipPlayerState,
    invitee: CoinflipPlayerState,
    invite_time: Instant,
    bid: i64,
    // state: GameState
}

#[derive(Clone, Debug)]
pub enum GameEvent {
    CoinflipAcceptEvent {
        invitee: u64,
        inviter: u64,
    },
    CoinflipCancelEvent {
        inviter: u64,
        invitee: u64,
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

// coinflip command
#[poise::command(
    slash_command,
    prefix_command,
    aliases("cf"),
    subcommands("send", "accept", "cancel"),
    subcommand_required
)]
pub async fn coinflip(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[poise::command(prefix_command, slash_command)]
pub async fn send(
    ctx: Context<'_>,

    #[description = "enemy"] opp: serenity::User,
    #[description = "bid"] bid: i64,
) -> Result<(), Error> {
    let db = ctx.data().database.clone();

    let chall_id = ctx.author().id.get();
    let opp_id = opp.id.get();

    let chall_bal = db.nigga_balance(chall_id).await?;
    let opp_bal = db.nigga_balance(opp_id).await?;

    if bid <= 0 {
        ctx.say("no negatives").await?;
        return Ok(());
    }

    if bid > chall_bal || bid > opp_bal {
        ctx.say("one of u doesnt have enough money for the bid")
            .await?;
        return Ok(());
    }

    ctx.say(format!(
        "sending coinflip inv to <@{}> with bid {} niggapoints, they got 30 seconds to accept",
        opp_id, bid
    ))
    .await?;

    let event_rx = ctx.data().event_rx.clone();
    let http = ctx.serenity_context().http.clone();
    let channel_id = ctx.channel_id();

    let mut game = CoinflipGame {
        inviter: CoinflipPlayerState {
            id: chall_id,
            accepted: true,
        },
        invitee: CoinflipPlayerState {
            id: opp_id,
            accepted: false,
        },
        invite_time: Instant::now(),
        bid, // state: GameState::Waiting,
    };

    let mut interval = time::interval(Duration::from_secs(1));

    tokio::spawn(async move {
        loop {
            interval.tick().await;

            // if invite isnt accepted within 30s quit
            if game.invite_time.elapsed() > Duration::from_secs(30) {
                channel_id
                    .say(
                        &http,
                        format!(
                            "coinflip inv by <@{}> against <@{}> expired",
                            chall_id, opp_id
                        ),
                    )
                    .await
                    .unwrap();
                break;
            }

            for event in event_rx.try_iter() {
                match event {
                    // a player accepts
                    GameEvent::CoinflipAcceptEvent { invitee, inviter }
                        if game.inviter.id == inviter && game.invitee.id == invitee =>
                    {
                        game.invitee.accepted = true
                    }

                    // a player cancels
                    GameEvent::CoinflipCancelEvent { inviter, invitee }
                        if game.inviter.id == inviter && game.invitee.id == invitee =>
                    {
                        game.invitee.accepted = false
                    }
                    _ => {}
                }
            }

            // if both cancel delete game

            if !game.inviter.accepted && !game.invitee.accepted {
                break;
            }

            // if both agree commence
            if game.inviter.accepted && game.invitee.accepted {
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
                    (game.inviter.id, game.invitee.id)
                } else {
                    (game.invitee.id, game.inviter.id)
                };

                channel_id
                    .say(&http, format!("<@{}>  WINS {} nigpoints", winner, game.bid))
                    .await
                    .unwrap();

                db.nigga_increment(winner, bid).await.unwrap();
                db.nigga_increment(loser, -bid).await.unwrap();

                break;
            }
        }
    });
    Ok(())
}

#[poise::command(prefix_command, slash_command)]
pub async fn accept(
    ctx: Context<'_>,
    #[description = "enemy"] challenger: serenity::User,
) -> Result<(), Error> {
    let accepter_id = ctx.author().id.get();
    let chall_id = challenger.id.get();
    ctx.data().event_tx.send(GameEvent::CoinflipAcceptEvent {
        invitee: accepter_id,
        inviter: chall_id,
    })?;
    Ok(())
}

#[poise::command(prefix_command, slash_command)]
pub async fn cancel(
    ctx: Context<'_>,
    #[description = "enemy"] opp: serenity::User,
) -> Result<(), Error> {
    let chall_id = ctx.author().id.get();
    let opp_id = opp.id.get();
    ctx.data().event_tx.send(GameEvent::CoinflipCancelEvent {
        inviter: chall_id,
        invitee: opp_id,
    })?;
    Ok(())
}
