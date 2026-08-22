use std::{fmt::Display, num::ParseIntError, str::FromStr};

use serenity::model::{
    application::{Interaction, ModalInteraction},
    id::UserId,
};

pub mod coinflip;
// pub mod poker;
pub mod poker_lobby;

pub struct EventWithData {
    pub event: GameEvent,
    pub game_id: GameId,
    pub user_id: UserId,
}

#[derive(PartialEq, Eq)]
pub struct GameId(u128);

#[derive(Clone, Debug)]
pub enum GameEvent {
    PokerLobbyJoin {
        funds: i64,
        interaction: ModalInteraction,
    },
    PokerLobbyLeave,
    CoinflipAccept {
        invitee: UserId,
        inviter: UserId,
    },
    CoinflipCancel {
        inviter: UserId,
        invitee: UserId,
    },
    PokerAccept {
        invitee: UserId,
        inviter: UserId,
        funds: i64,
    },
    PokerCancel {
        invitee: UserId,
        inviter: UserId,
    },
    PokerCall {
        caller: UserId,
    },
    PokerRaise {
        raiser: UserId,
        amount: i64,
    },
    PokerFold {
        folder: UserId,
    },
    PokerCheck {
        checker: UserId,
    },
}

impl GameId {
    pub fn new() -> Self {
        Self(rand::random())
    }
}

impl Display for GameId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:x}", self.0)
    }
}

impl FromStr for GameId {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(u128::from_str_radix(s, 16)?))
    }
}
