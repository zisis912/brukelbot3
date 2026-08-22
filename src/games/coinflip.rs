use std::{sync::Arc, time::Duration};

use crossbeam::channel::Receiver;
use serenity::{
    all::prelude::Mentionable,
    http::Http,
    model::id::{ChannelId, UserId},
};
use tokio::time::{self, Instant};

use crate::{database::Database, games::GameEvent};

pub struct CoinflipPlayerState {
    pub id: UserId,
    pub accepted: bool,
}

pub struct CoinflipGame {
    pub inviter: CoinflipPlayerState,
    pub invitee: CoinflipPlayerState,
    pub invite_time: Instant,
    pub bid: i64,
    // state: GameState
}

impl CoinflipGame {
    pub fn start(
        mut self,
        db: Database,
        channel_id: ChannelId,
        http: Arc<Http>,
        event_rx: Receiver<GameEvent>,
    ) {
        let mut interval = time::interval(Duration::from_secs(1));

        tokio::spawn(async move {
            loop {
                interval.tick().await;

                // if invite isnt accepted within 30s quit
                if self.invite_time.elapsed() > Duration::from_secs(30) {
                    channel_id
                        .say(
                            &http,
                            format!(
                                "coinflip inv by {} against {} expired",
                                self.inviter.id.mention(),
                                self.invitee.id.mention()
                            ),
                        )
                        .await
                        .unwrap();
                    break;
                }

                for event in event_rx.try_iter() {
                    match event {
                        // a player accepts
                        GameEvent::CoinflipAccept { invitee, inviter }
                            if self.inviter.id == inviter && self.invitee.id == invitee =>
                        {
                            self.invitee.accepted = true
                        }

                        // a player cancels
                        GameEvent::CoinflipCancel { inviter, invitee }
                            if self.inviter.id == inviter && self.invitee.id == invitee =>
                        {
                            self.invitee.accepted = false
                        }
                        _ => {}
                    }
                }

                // if both cancel delete self

                if !self.inviter.accepted && !self.invitee.accepted {
                    break;
                }

                // if both agree commence
                if self.inviter.accepted && self.invitee.accepted {
                    channel_id
                        .say(
                            &http,
                            format!(
                                "COINFLIP BATTLE:\n {} against {} STARTS",
                                self.inviter.id.mention(),
                                self.invitee.id.mention()
                            ),
                        )
                        .await
                        .unwrap();

                    let p1_wins = rand::random_bool(0.5);
                    let (winner, loser) = if p1_wins {
                        (self.inviter.id, self.invitee.id)
                    } else {
                        (self.invitee.id, self.inviter.id)
                    };

                    channel_id
                        .say(
                            &http,
                            format!("{}  WINS {} nigpoints", winner.mention(), self.bid),
                        )
                        .await
                        .unwrap();

                    db.nigga_increment(winner, self.bid).await.unwrap();
                    db.nigga_increment(loser, -self.bid).await.unwrap();

                    break;
                }
            }
        });
    }
}
