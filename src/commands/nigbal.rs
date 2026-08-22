use crate::Context;
use crate::Error;
use crate::serenity;

#[poise::command(slash_command, prefix_command)]
pub async fn nigbal(
    ctx: Context<'_>,
    #[description = "user"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let user = user.map(|u| u.id).unwrap_or(ctx.author().id);
    let balance = ctx.data().database.nigga_balance(user).await.unwrap();

    ctx.say(format!("{balance}")).await?;

    Ok(())
}
