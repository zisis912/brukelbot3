use ::serenity::all::prelude::Mentionable;
use tokio::time::Instant;

use crate::games::GameEvent;
use crate::games::coinflip::{CoinflipGame, CoinflipPlayerState};
use crate::serenity;
use crate::{Context, Error};

// use crate::{
//     CommandError::{self, InvalidArgCount},
//     Handler, nigga_increment,
// };

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

    let chall_id = ctx.author().id;
    let opp_id = opp.id;

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
        "sending coinflip inv to {} with bid {} niggapoints, they got 30 seconds to accept",
        opp_id.mention(),
        bid
    ))
    .await?;

    let event_rx = ctx.data().event_rx.clone();
    let http = ctx.serenity_context().http.clone();
    let channel_id = ctx.channel_id();

    let game = CoinflipGame {
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

    game.start(db, channel_id, http, event_rx);

    Ok(())
}

#[poise::command(prefix_command, slash_command)]
pub async fn accept(
    ctx: Context<'_>,
    #[description = "enemy"] challenger: serenity::User,
) -> Result<(), Error> {
    let accepter_id = ctx.author().id;
    let chall_id = challenger.id;
    ctx.data().event_tx.send(GameEvent::CoinflipAccept {
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
    let chall_id = ctx.author().id;
    let opp_id = opp.id;
    ctx.data().event_tx.send(GameEvent::CoinflipCancel {
        inviter: chall_id,
        invitee: opp_id,
    })?;
    Ok(())
}
