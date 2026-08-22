use crate::Context;
use crate::Error;
use crate::serenity;

#[poise::command(slash_command, prefix_command)]
pub async fn nigpay(
    ctx: Context<'_>,
    #[description = "Recipient"] nigrecipient: serenity::User,
    #[description = "count"] nigcount: i64,
) -> Result<(), Error> {
    let nigsender = ctx.author().id;

    if nigcount <= 0 {
        ctx.say("no negatives").await?;
        return Ok(());
    }

    let db = &ctx.data().database;

    let nigbal = db.nigga_balance(nigsender).await?;

    if nigcount > nigbal {
        ctx.say("you dont have that much").await?;
        return Ok(());
    }

    db.nigga_increment(nigrecipient.id, nigcount).await?;
    db.nigga_increment(nigsender, -nigcount).await?;

    ctx.say("sent").await?;

    Ok(())
}
