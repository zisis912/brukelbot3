use crate::games::poker_lobby::PokerLobby;
use crate::{Error, serenity};
use ::serenity::all::prelude::Mentionable;
use ::serenity::model::id::UserId;

use crate::Context;

// poker command
#[poise::command(
    slash_command,
    prefix_command,
    // aliases("cf"),
    // subcommands("send", "accept", "cancel", "raise", "fold", "check"),
    subcommands("send"),
    // subcommand_required
)]
pub async fn poker(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

// #[poise::command(prefix_command, slash_command)]
// pub async fn fold(ctx: Context<'_>) -> Result<(), Error> {
//     let folder = ctx.author().id;
//     ctx.data().event_tx.send(GameEvent::PokerFold { folder })?;
//     Ok(())
// }
//
// #[poise::command(prefix_command, slash_command)]
// pub async fn raise(
//     ctx: Context<'_>,
//
//     #[description = "How much money to raise by"] amount: i64,
// ) -> Result<(), Error> {
//     let raiser = ctx.author().id;
//
//     if amount <= 0 {
//         ctx.say("cant raise 0 or less").await?;
//     }
//
//     ctx.data()
//         .event_tx
//         .send(GameEvent::PokerRaise { raiser, amount })?;
//     Ok(())
// }

#[poise::command(prefix_command, slash_command)]
pub async fn send(
    ctx: Context<'_>,

    #[description = "ante"] ante: i64,
    #[description = "funds"] funds: i64,
    #[description = "Opponents to invite"] enemies: Vec<serenity::User>,
) -> Result<(), Error> {
    // max 6 players (5 others)
    if enemies.len() > 5 || enemies.is_empty() {
        ctx.say("total players must be between 2 and 6").await?;
        return Ok(());
    }

    if ante < 0 {
        ctx.say("ante cant be negative").await?;
        return Ok(());
    }

    // make sure theres no sign overflow
    if funds <= 0 {
        ctx.say("funds must be positive").await?;
        return Ok(());
    }

    // let challenger_funds = funds;
    //
    let challenger = ctx.author().id;

    let db = ctx.data().database.clone();

    // convert users to IDs, add the challenger himself
    let mut invitees: Vec<UserId> = enemies.iter().map(|e| e.id).collect();
    invitees.push(challenger);

    // make sure everybody can pay ante
    for &userid in &invitees {
        if db.nigga_balance(userid).await.unwrap() < ante {
            ctx.say(format!("error: {} cant pay the ante", userid.mention()))
                .await?;
            return Ok(());
        }
    }

    ctx.say(format!(
        "poker game invite with {ante} ante sent, waiting for accept! (expires in 30s)",
    ))
    .await?;

    let event2_rx = ctx.data().event2_rx.clone();
    let http = ctx.serenity_context().http.clone();
    let channel_id = ctx.channel_id();

    // let game = PokerGame::new(challenger, funds, invitees, ante);
    // game.start(db, channel_id, http, event_rx);

    let lobby = PokerLobby::new(invitees, ante);
    lobby.start(db, channel_id, http, event2_rx).await;

    Ok(())
}

// #[poise::command(prefix_command, slash_command)]
// pub async fn accept(
//     ctx: Context<'_>,
//
//     #[description = "challenger"] chall: serenity::User,
//     #[description = "Funds you will be entering with"] funds: i64,
// ) -> Result<(), Error> {
//     let accepter_id = ctx.author().id;
//     let chall_id = chall.id;
//
//     ctx.data().event_tx.send(GameEvent::PokerAccept {
//         invitee: accepter_id,
//         inviter: chall_id,
//         funds,
//     })?;
//     Ok(())
// }
//
// #[poise::command(prefix_command, slash_command)]
// pub async fn cancel(
//     ctx: Context<'_>,
//     #[description = "inviter"] inviter: serenity::User,
// ) -> Result<(), Error> {
//     let invitee = ctx.author().id;
//     let inviter = inviter.id;
//     ctx.data()
//         .event_tx
//         .send(GameEvent::PokerCancel { invitee, inviter })?;
//     Ok(())
// }

// #[poise::command(prefix_command, slash_command)]
// pub async fn check(ctx: Context<'_>) -> Result<(), Error> {
//     let checker = ctx.author().id;
//     ctx.data()
//         .event_tx
//         .send(GameEvent::PokerCheck { checker })?;
//     Ok(())
// }
//
// #[poise::command(prefix_command, slash_command)]
// pub async fn call(ctx: Context<'_>) -> Result<(), Error> {
//     let caller = ctx.author().id;
//     ctx.data().event_tx.send(GameEvent::PokerCall { caller })?;
//     Ok(())
// }
