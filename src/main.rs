use std::str::FromStr;

use crate::games::{EventWithData, GameEvent, GameId};
use ::serenity::client::FullEvent;
use ::serenity::model::application::Interaction;
use crossbeam::channel::{Receiver, Sender};
use regex::Regex;
use serenity::all::{CreateAttachment, CreateMessage};
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use thiserror::Error;

use crate::database::Database;

pub use poise::serenity_prelude as serenity;
use poise::{Framework, FrameworkContext};
pub type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

mod commands;
mod database;
mod events;
mod games;

// #[derive(Error, Debug)]
// pub enum CommandError {
//     #[error("invalid arg count")]
//     InvalidArgCount,
//     #[error("failed to parse int")]
//     IntParsingError(#[from] ParseIntError),
//     #[error("invalid user")]
//     InvalidUser,
//     #[error("database error")]
//     DatabaseError(#[from] sqlx::Error),
//     #[error("serenity error")]
//     SerenityError(#[from] serenity::Error),
//     #[error("sign overflow")]
//     SignedIntErr(#[from] TryFromIntError),
//     #[error("sign overflow")]
//     EventDispatchErr(#[from] error::SendError<GameEvent>),
// }

// #[async_trait]
// impl EventHandler for Handler {

// Set a handler for the `message` event. This is called whenever a new message is received.
//
// Event handlers are dispatched through a threadpool, and so multiple events can be
// dispatched simultaneously.

async fn event_handler(
    ctx: &serenity::Context,
    event: &FullEvent,
    framework: FrameworkContext<'_, Data, Error>,
) -> Result<(), Error> {
    match event {
        FullEvent::Message { new_message: msg } => {
            // i assume cast from usize-> u64 is safe
            let nigga_count = framework
                .user_data
                .nigga_regex
                .find_iter(&msg.content)
                .count() as u64;

            if nigga_count > 0 {
                framework
                    .user_data
                    .database
                    .nigga_increment(msg.author.id, nigga_count.try_into().unwrap_or(i64::MAX))
                    .await
                    .unwrap();
            }

            // ohm will cause self recursion
            if msg.author.bot {
                return Ok(());
            }

            if msg.content.to_lowercase().contains("ohm") {
                // Sending a message can fail, due to a network error, an authentication error, or lack
                // of permissions to post in the channel, so log to stdout when some error happens,
                // with a description of it.
                let attachment =
                    CreateAttachment::bytes(include_bytes!("../assets/ohm.png"), "ohm.png");

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
        }
        FullEvent::InteractionCreate {
            interaction: root_interaction,
        } => match root_interaction {
            Interaction::Component(interaction) => {
                let Some(GameData { game, game_id }) = parse_game_data(&interaction.data.custom_id)
                else {
                    return Ok(());
                };

                match game.as_str() {
                    "pokerlobbyjoin" => {
                        let response = serenity::CreateInteractionResponse::Modal(
                            serenity::CreateModal::new(
                                // modal response will have the exact same customid as the button
                                // response
                                interaction.data.custom_id.clone(),
                                "join poker game",
                            )
                            .components(vec![
                                serenity::CreateActionRow::InputText(
                                    serenity::CreateInputText::new(
                                        serenity::InputTextStyle::Short,
                                        "Funds (excluding ante)", // label
                                        "funds",                  // custom_id
                                    )
                                    .placeholder("The amount of money you bring to the table")
                                    .required(true),
                                ),
                            ]),
                        );

                        interaction.create_response(&ctx.http, response).await?;
                    }
                    "pokerlobbyleave" => {
                        framework
                            .user_data
                            .event2_tx
                            .send(EventWithData {
                                game_id,
                                event: GameEvent::PokerLobbyLeave,
                                user_id: interaction.user.id,
                            })
                            .unwrap();
                    }
                    _ => return Ok(()),
                };
            }
            Interaction::Modal(interaction) => {
                let Some(GameData { game, game_id }) = parse_game_data(&interaction.data.custom_id)
                else {
                    return Ok(());
                };

                match game.as_str() {
                    "pokerlobbyjoin" => {
                        if let Some(funds) = interaction.data.components.iter().find_map(|row| {
                            row.components.iter().find_map(|component| {
                                if let serenity::ActionRowComponent::InputText(input) = component {
                                    match input.custom_id.as_str() {
                                        "funds" => {
                                            // safety: funds is required
                                            let value = input.value.clone().unwrap();
                                            // if this return None, funds will be none
                                            value.parse::<i64>().ok()
                                        }

                                        _ => None,
                                    }
                                } else {
                                    None
                                }
                            })
                        }) {
                            framework.user_data.event2_tx.send(EventWithData {
                                game_id,
                                user_id: interaction.user.id,
                                event: GameEvent::PokerLobbyJoin {
                                    funds,
                                    interaction: interaction.clone(),
                                },
                            })?;
                            interaction.defer_ephemeral(&ctx.http).await?;
                        } else {
                            interaction
                                .create_response(
                                    ctx,
                                    serenity::CreateInteractionResponse::Message(
                                        serenity::CreateInteractionResponseMessage::new()
                                            .content("funds must be a number")
                                            .ephemeral(false),
                                    ),
                                )
                                .await?;
                        }
                    }
                    _ => return Ok(()),
                };

                // interaction.create_response(&ctx.http, response).await?;
            }
            _ => {}
        },
        _ => {}
    }
    Ok(())
}

fn on_ready(ready: &Ready) {
    println!("{} is connected!", ready.user.name);
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    // this is only useful for testing outside of docker
    dotenvy::dotenv().ok();

    let token = std::env::var("DISCORD_TOKEN").expect("no discord token");

    let database = Database::new().await?;

    let (event_tx, event_rx) = crossbeam::channel::unbounded();
    let (event2_tx, event2_rx) = crossbeam::channel::unbounded();

    let data = Data {
        nigga_regex: Regex::new(r"(?i)nigger|nigga").unwrap(),
        database,
        event_tx,
        event_rx,
        event2_tx,
        event2_rx,
    };

    let framework: Framework<Data, Error> = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::coinflip::coinflip(),
                commands::nigpay::nigpay(),
                commands::leaderboard::leaderboard(),
                commands::poker::poker(),
                commands::nigbal::nigbal(),
            ],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some(";".into()),
                non_command_message: Some(|_, _, _msg| {
                    Box::pin(async move {
                        // println!("non command message!: {}", msg.content);
                        Ok(())
                    })
                }),
                ..Default::default()
            },
            on_error: |error| {
                Box::pin(async move {
                    println!("what the hell");
                    match error {
                        poise::FrameworkError::ArgumentParse { error, .. } => {
                            if let Some(error) = error.downcast_ref::<serenity::RoleParseError>() {
                                println!("Found a RoleParseError: {:?}", error);
                            } else {
                                println!("Not a RoleParseError :(");
                            }
                        }
                        other => poise::builtins::on_error(other).await.unwrap(),
                    }
                })
            },
            event_handler: |ctx, event, framework, _| {
                Box::pin(event_handler(ctx, event, framework))
            },
            ..Default::default()
        })
        .setup(move |ctx, ready, framework| {
            on_ready(ready);
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(data)
            })
        })
        .build();

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::all();

    // Create a new instance of the Client, logging in as a bot. This will automatically prepend
    // your bot token with "Bot ", which is a requirement by Discord for bot users.
    let mut client = serenity::ClientBuilder::new(token, intents)
        // .event_handler(Handler)
        .framework(framework)
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

struct Data {
    database: Database,
    event_tx: Sender<GameEvent>,
    event_rx: Receiver<GameEvent>,
    event2_tx: Sender<EventWithData>,
    event2_rx: Receiver<EventWithData>,
    nigga_regex: Regex,
}

struct GameData {
    game: String,
    game_id: GameId,
}

fn parse_game_data(game_data: &str) -> Option<GameData> {
    let Some((name, id_str)) = game_data.split_once('_') else {
        return None;
    };
    let Ok(id) = GameId::from_str(id_str) else {
        return None;
    };

    Some(GameData {
        game: name.to_owned(),
        game_id: id,
    })
}
