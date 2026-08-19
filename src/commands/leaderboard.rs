use ::serenity::builder::CreateEmbed;
use poise::CreateReply;

use crate::Context;
use crate::Error;
use crate::database::Ranking;

#[poise::command(slash_command, prefix_command, aliases("lb"))]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    let mut rankings: Vec<Ranking> = ctx.data().database.get_rankings().await.unwrap();

    rankings.sort_by(|a, b| b.count.cmp(&a.count));

    let description: String = rankings
        .iter()
        .take(10)
        .enumerate()
        .map(|(idx, Ranking { id, count })| format!("- #{}: <@{}> {} niggas", idx + 1, id, count))
        .collect::<Vec<String>>()
        .join("\n");

    let embed = CreateEmbed::new()
        .title("NIGGA LEADERBOARD")
        .description(description);

    let builder = CreateReply::default().embed(embed);

    let _ = ctx.send(builder).await?;
    Ok(())
}
