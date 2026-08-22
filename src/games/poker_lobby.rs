use std::{collections::HashSet, sync::Arc, time::Duration};

use ::serenity::{
    all::prelude::Mentionable,
    builder::{CreateEmbed, CreateInteractionResponseFollowup, CreateMessage, EditMessage},
};
use crossbeam::channel::Receiver;
use serenity::{
    http::Http,
    model::id::{ChannelId, UserId},
};
use tokio::time::{self, Instant};

use crate::{
    database::Database,
    games::{GameEvent, GameId},
};
use crate::{games::EventWithData, serenity};

pub struct PokerLobby {
    game_id: GameId,
    creation_date: Instant,
    invitees: Vec<InviteeStatus>,
    ante: i64,
}

struct InviteeStatus {
    id: UserId,
    funds: Option<i64>,
}

impl PokerLobby {
    pub fn new(invitees: Vec<UserId>, ante: i64) -> Self {
        let invitees = invitees
            .iter()
            .copied()
            .map(|id| InviteeStatus { id, funds: None })
            .collect();

        Self {
            creation_date: Instant::now(),
            ante,
            game_id: GameId::new(),
            invitees,
        }
    }

    pub async fn start(
        mut self,
        db: Database,
        channel_id: ChannelId,
        http: Arc<Http>,
        event2_rx: Receiver<EventWithData>,
    ) {
        if self.invitees.len()
            != self
                .invitees
                .iter()
                .map(|i| i.id)
                .collect::<HashSet<_>>()
                .len()
        {
            channel_id
                .send_message(
                    &http,
                    serenity::CreateMessage::new().content("retard dont use duplicates"),
                )
                .await
                .unwrap();
            return;
        }

        if self.invitees.len() < 2 || self.invitees.len() > 6 {
            channel_id
                .send_message(
                    &http,
                    serenity::CreateMessage::new().content("only 2-6 players can play"),
                )
                .await
                .unwrap();
            return;
        }

        let components = vec![serenity::CreateActionRow::Buttons(vec![
            serenity::CreateButton::new(format!("pokerlobbyjoin_{}", self.game_id))
                .label("join lobby")
                .style(serenity::ButtonStyle::Primary),
            serenity::CreateButton::new(format!("pokerlobbyleave_{}", self.game_id))
                .label("leave lobby")
                .style(serenity::ButtonStyle::Primary),
        ])];

        let msg = serenity::CreateMessage::new()
            .content("nate higgers")
            .embed(self.get_embed())
            .components(components);

        let mut lobby_message = channel_id.send_message(&http, msg).await.unwrap();

        let mut interval = time::interval(Duration::from_millis(100));

        loop {
            interval.tick().await;
            if self.can_start() {
                channel_id
                    .send_message(&http, CreateMessage::new().content("Starting game"))
                    .await
                    .unwrap();
                break;
            }

            while let Ok(EventWithData {
                event,
                game_id,
                user_id,
            }) = event2_rx.try_recv()
            {
                if self.game_id != game_id {
                    continue;
                }

                match event {
                    GameEvent::PokerLobbyJoin { funds, interaction } => {
                        let Some(invitee) = self.invitees.iter_mut().find(|i| i.id == user_id)
                        else {
                            interaction
                                .create_followup(
                                    &http,
                                    CreateInteractionResponseFollowup::new()
                                        .content("you werent invited retard"),
                                )
                                .await
                                .unwrap();
                            continue;
                        };

                        if funds <= 0 {
                            interaction
                                .create_followup(
                                    &http,
                                    CreateInteractionResponseFollowup::new()
                                        .content("no negatives"),
                                )
                                .await
                                .unwrap();
                            continue;
                        }

                        if db.nigga_balance(user_id).await.unwrap() < self.ante + funds {
                            interaction
                                .create_followup(
                                    &http,
                                    CreateInteractionResponseFollowup::new()
                                        .content("you dont have enough money to pay ante+funds"),
                                )
                                .await
                                .unwrap();
                            continue;
                        }

                        invitee.funds = Some(funds);
                        interaction
                            .create_followup(
                                &http,
                                CreateInteractionResponseFollowup::new().content("joined lobby!"),
                            )
                            .await
                            .unwrap();

                        lobby_message
                            .edit(&http, EditMessage::new().embed(self.get_embed()))
                            .await
                            .unwrap();
                    }
                    GameEvent::PokerLobbyLeave => {
                        self.invitees.retain(|i| i.id != user_id);
                    }
                    _ => {}
                }
            }
        }
    }

    fn get_embed(&self) -> CreateEmbed {
        let embed = CreateEmbed::new()
            .title("POKER LOBBY")
            .description(format!("Ante: {} niggapoints", self.ante))
            .field(
                "Players:",
                self.invitees
                    .iter()
                    .map(get_formatted_player)
                    .collect::<String>(),
                false,
            );
        // .footer(CreateEmbedFooter::new("uwuslayer feet"));
        embed
    }

    fn can_start(&self) -> bool {
        self.invitees.iter().all(|invitee| invitee.funds.is_some())
    }
}

fn get_formatted_player(invitee: &InviteeStatus) -> String {
    let (icon, suffix) = match invitee.funds {
        Some(funds) => ('✅', format!(" (funds: {funds} nig)")),
        None => ('❌', "".to_owned()),
    };

    format!("{icon}{}{suffix}\n", invitee.id.mention(),)
}
