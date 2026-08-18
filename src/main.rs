use std::num::{ParseIntError, TryFromIntError};

use dotenvy_macro::dotenv;
use regex::Regex;
use serenity::all::{CreateAttachment, CreateEmbed, CreateMessage};
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use sqlx::SqlitePool;
use thiserror::Error;
use tokio::sync::broadcast::{self, Receiver, Sender};

use crate::coinflip::GameEvent;

mod coinflip;
mod poker;

struct Handler {
    pool: SqlitePool,
    nigga_regex: Regex,
    user_regex: Regex,
    channel_send: Sender<GameEvent>,
    // channel_recv: Receiver<GameEvent>,
}

#[derive(Error, Debug)]
pub enum CommandError {
    #[error("invalid arg count")]
    InvalidArgCount,
    #[error("failed to parse int")]
    IntParsingError(#[from] ParseIntError),
    #[error("invalid user")]
    InvalidUser,
    #[error("database error")]
    DatabaseError(#[from] sqlx::Error),
    #[error("serenity error")]
    SerenityError(#[from] serenity::Error),
    #[error("sign overflow")]
    SignedIntErr(#[from] TryFromIntError),
    #[error("sign overflow")]
    EventDispatchErr(#[from] broadcast::error::SendError<GameEvent>),
    // #[error("insufficient funds")]
    // InsufficientFunds,
    // InsufficientArgs(#[from] io::Error),
    // #[error("the data for key `{0}` is not available")]
    // Redaction(String),
    // #[error("invalid header (expected {expected:?}, found {found:?})")]
    // InvalidHeader {
    //     expected: String,
    //     found: String,
    // },
    // #[error("unknown data store error")]
    // Unknown,
}

async fn nigga_increment(
    pool: &SqlitePool,
    user_id: u64,
    increment: i64,
) -> Result<(), sqlx::Error> {
    println!("{},{}", user_id, increment);
    sqlx::query!(
        "INSERT INTO nigga_leaderboard VALUES(?,?)
                ON CONFLICT(user_id) DO UPDATE SET nigga_count = nigga_count + ?;",
        user_id as i64,
        increment,
        increment
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn nigga_balance(pool: &SqlitePool, user_id: u64) -> Result<i64, sqlx::Error> {
    let nigbal: i64 = sqlx::query!(
        r#"SELECT nigga_count AS "nigbal!" FROM nigga_leaderboard WHERE user_id = ?;"#,
        user_id as i64
    )
    .fetch_optional(pool)
    .await?
    .map(|r| r.nigbal)
    .unwrap_or(0);

    Ok(nigbal)
}

impl Handler {
    async fn nigga_increment(&self, user_id: u64, increment: i64) -> Result<(), sqlx::Error> {
        nigga_increment(&self.pool, user_id, increment).await
    }

    async fn nigga_balance(&self, user_id: u64) -> Result<i64, sqlx::Error> {
        nigga_balance(&self.pool, user_id).await
    }

    async fn nigpay(
        &self,
        msg: &Message,
        ctx: &Context,
        args: Vec<&str>,
    ) -> Result<(), CommandError> {
        // TODO: find better way to write this?
        let [recipient, count] = get_args(args)?;

        let nigrecipient = self.resolve_user_mention(recipient)?;
        let nigcount: i64 = count.parse()?;
        let nigsender = msg.author.id.get();

        if nigcount <= 0 {
            msg.channel_id.say(&ctx.http, "no negatives").await?;
            return Ok(());
        }

        let nigbal = self.nigga_balance(nigsender).await?;

        if nigcount > nigbal {
            msg.channel_id
                .say(&ctx.http, "you dont have that much")
                .await?;
            return Ok(());
        }

        self.nigga_increment(nigrecipient, nigcount).await?;
        self.nigga_increment(nigsender, -nigcount).await?;

        msg.channel_id.say(&ctx.http, "sent").await?;

        Ok(())
    }

    fn resolve_user_mention(&self, recipient: &str) -> Result<u64, CommandError> {
        if !self.user_regex.is_match(recipient) {
            return Err(CommandError::InvalidUser);
        }
        let nigrecipient: u64 = recipient[2..recipient.len() - 1].parse()?;
        Ok(nigrecipient)
    }
}

fn get_args<const N: usize>(args: Vec<&str>) -> Result<[&str; N], CommandError> {
    TryInto::<[&str; N]>::try_into(args).map_err(|_| CommandError::InvalidArgCount)
}

#[async_trait]
impl EventHandler for Handler {
    // Set a handler for the `message` event. This is called whenever a new message is received.
    //
    // Event handlers are dispatched through a threadpool, and so multiple events can be
    // dispatched simultaneously.
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        let nigga_count = self.nigga_regex.find_iter(&msg.content).count();
        if nigga_count > 0 {
            self.nigga_increment(msg.author.id.get(), nigga_count as i64)
                .await
                .unwrap();
        }

        if msg.content.to_lowercase().contains("ohm") {
            // Sending a message can fail, due to a network error, an authentication error, or lack
            // of permissions to post in the channel, so log to stdout when some error happens,
            // with a description of it.
            let attachment = CreateAttachment::path("./assets/ohm.png").await.unwrap();
            let builder = CreateMessage::new()
                .content("ohm")
                .tts(false)
                .add_file(attachment);

            if let Err(why) = msg.channel_id.send_message(&ctx.http, builder).await {
                println!("Error sending message: {why:?}");
            }
        }

        if msg.content == "say hi" {
            if let Err(why) = msg.channel_id.say(&ctx.http, "hi").await {
                println!("Error sending message: {why:?}");
            }
        }

        // COMMANDS

        if !msg.content.starts_with(';') {
            return;
        }

        let mut split = msg.content[1..].split(" ").filter(|str| !str.is_empty());
        let Some(command) = split.nth(0) else {
            return;
        };
        let args: Vec<&str> = split.collect();

        if command == "leaderboard" || command == "lb" {
            struct Ranking {
                id: i64,
                count: i64,
            }
            let mut rankings: Vec<Ranking> = sqlx::query_as!(
                Ranking,
                r#"SELECT user_id AS "id!", nigga_count AS "count!" FROM nigga_leaderboard;"#
            )
            .fetch_all(&self.pool)
            .await
            .unwrap();

            rankings.sort_by(|a, b| b.count.cmp(&a.count));

            let description: String = rankings
                .iter()
                .take(10)
                .enumerate()
                .map(|(idx, Ranking { id, count })| {
                    format!("- #{}: <@{}> {} niggas", idx + 1, id, count)
                })
                .collect::<Vec<String>>()
                .join("\n");

            let embed = CreateEmbed::new()
                .title("NIGGA LEADERBOARD")
                .description(description);

            let builder = CreateMessage::new().embed(embed);

            if let Err(why) = msg.channel_id.send_message(&ctx.http, builder).await {
                println!("Error sending message: {why:?}");
            }
        }

        if command == "nigpay" {
            self.nigpay(&msg, &ctx, args.clone()).await.unwrap();
        }

        if command == "coinflip" || command == "cf" {
            self.coinflip(&msg, &ctx, args.clone()).await.unwrap();
        }

        if command == "poker" {
            self.poker(&msg, &ctx, args.clone()).await.unwrap();
        }
    }

    // Set a handler to be called on the `ready` event. This is called when a shard is booted, and
    // a READY payload is sent by Discord. This payload contains data like the current user's guild
    // Ids, current user data, private channels, and more.
    //
    // In this case, just print what the current user's username is.
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // this is only useful for testing outside of docker
    dotenvy::dotenv().ok();

    let token = std::env::var("DISCORD_TOKEN").expect("no discord token");

    let sql_pool = SqlitePool::connect("sqlite://data/database.db?mode=rwc")
        .await
        .unwrap();

    sqlx::migrate!("./migrations").run(&sql_pool).await?;

    let (channel_send, _) = broadcast::channel(128);

    let handler = Handler {
        pool: sql_pool,
        nigga_regex: Regex::new(r"(?i)nigger|nigga").unwrap(),
        user_regex: Regex::new(r"<@\d{1,20}>").unwrap(),
        channel_send,
        // channel_recv,
    };

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::all();

    // Create a new instance of the Client, logging in as a bot. This will automatically prepend
    // your bot token with "Bot ", which is a requirement by Discord for bot users.
    let mut client = Client::builder(token, intents)
        .event_handler(handler)
        .await
        .expect("Err creating client");

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform exponential backoff until
    // it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }

    Ok(())
}
