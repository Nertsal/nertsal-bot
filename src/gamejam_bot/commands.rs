use rand::seq::SliceRandom;
use std::sync::Arc;

use super::*;

impl CommandBot<Self> for GameJamBot {
    fn get_commands(&self) -> &BotCommands<Self> {
        &self.commands
    }
}

impl GameJamBot {
    fn set_current(&mut self, game: Option<Game>) -> Option<String> {
        self.time_limit = None;

        match self.games_state.current_game.take() {
            Some(game) => {
                self.played_games.push(game.clone());
                save_into(&self.played_games, &self.played_games_file).unwrap();
            }
            None => (),
        }

        let reply = match game {
            Some(game) => {
                self.update_status(&format!("Playing {} from {}", game.name, game.author));
                let reply = format!("Now playing {} from @{}. ", game.name, game.author);
                self.games_state.raffle.viewers_weight.remove(&game.author);
                self.games_state.current_game = Some(game);
                Some(reply)
            }
            None => {
                self.update_status("Not playing a game");
                self.games_state.current_game = None;
                None
            }
        };

        self.save_games().unwrap();
        reply
    }
    fn remove_game(&mut self, author_name: &str) -> Option<Game> {
        let pos = self
            .games_state
            .returned_queue
            .iter()
            .enumerate()
            .find(|&(_, game)| game.author == author_name)
            .map(|(pos, _)| pos);
        pos.map(|pos| self.games_state.returned_queue.remove(pos))
            .or_else(|| {
                let pos = self
                    .games_state
                    .games_queue
                    .iter()
                    .enumerate()
                    .find(|&(_, game)| game.author == author_name)
                    .map(|(pos, _)| pos);
                pos.map(|pos| self.games_state.games_queue.remove(pos))
            })
            .flatten()
            .or_else(|| {
                let pos = self
                    .games_state
                    .skipped
                    .iter()
                    .enumerate()
                    .find(|&(_, game)| game.author == author_name)
                    .map(|(pos, _)| pos);
                pos.map(|pos| self.games_state.skipped.remove(pos))
            })
    }
    fn remove_game_response(&mut self, author_name: &str) -> Option<String> {
        match self.remove_game(author_name) {
            Some(_) => {
                let reply = format!("{}'s game has been removed from the queue", author_name);
                Some(reply)
            }
            None => {
                let reply = format!("Couldn't find a game from {}", author_name);
                Some(reply)
            }
        }
    }
    pub fn next(
        &mut self,
        author_name: Option<String>,
        confirmation_required: bool,
    ) -> Option<String> {
        let game = match &author_name {
            Some(author_name) => match self.remove_game(author_name) {
                Some(game) => Ok(game),
                None => Err(format!("Couldn't find a game from {}", author_name)),
            },
            None => match self
                .games_state
                .returned_queue
                .pop_front()
                .or_else(|| self.games_state.games_queue.pop_front())
            {
                Some(game) => Ok(game),
                None => Err(format!("The queue is empty. !submit <your game>. ")),
            },
        };

        let reply = match game {
            Ok(game) => {
                let game_author = game.author.clone();
                let mut reply = self.set_current(Some(game));
                if confirmation_required {
                    if let Some(response_time) = self.config.response_time_limit {
                        self.time_limit = Some(response_time as f32);
                        self.update_status(&format!("Waiting for response from {}", game_author));
                        reply = Some(format!(
                            "@{}, we are about to play your game. Please reply in {} seconds.",
                            game_author, response_time
                        ))
                    }
                }
                reply
            }
            Err(reply) => {
                self.set_current(None);
                Some(reply)
            }
        };
        self.save_games().unwrap();
        reply
    }
    pub fn skip(&mut self, auto_next: bool) -> Option<String> {
        self.time_limit = None;
        match self.games_state.current_game.take() {
            Some(game) => {
                self.games_state.skipped.push(game);
                let reply = "Game has been skipped.".to_owned();
                let reply = if auto_next {
                    self.next(None, true).unwrap_or(reply)
                } else {
                    self.save_games().unwrap();
                    reply
                };
                Some(reply)
            }
            None => Some("Not playing any game at the moment.".to_owned()),
        }
    }
    fn skip_all(&mut self) -> Option<String> {
        // self.skip(false);
        for game in self.games_state.returned_queue.drain(..) {
            self.games_state.skipped.push(game);
        }
        for game in self.games_state.games_queue.drain(..) {
            self.games_state.skipped.push(game);
        }
        self.save_games().unwrap();
        Some(format!(
            "All games from the queue are moved to the skipped list."
        ))
    }
    fn unskip(&mut self, author_name: Option<String>) -> Option<String> {
        let mut reply = String::new();
        if let Some(current) = self.games_state.current_game.take() {
            self.games_state.returned_queue.push_front(current);
            reply.push_str("Current game has been put at the front of the queue. ");
        }

        match author_name {
            Some(author_name) => {
                let skipped = self
                    .games_state
                    .skipped
                    .iter()
                    .enumerate()
                    .find_map(|(index, game)| {
                        if game.author == author_name {
                            Some(index)
                        } else {
                            None
                        }
                    })
                    .map(|index| self.games_state.skipped.remove(index));
                match self.set_current(skipped) {
                    Some(set_reply) => reply.push_str(&set_reply),
                    None => reply.push_str(&format!("No game from {} found", author_name)),
                }
            }
            None => {
                if self.games_state.skipped.len() > 0 {
                    let skipped = self
                        .games_state
                        .skipped
                        .remove(self.games_state.skipped.len() - 1);
                    match self.set_current(Some(skipped)) {
                        Some(set_reply) => reply.push_str(&set_reply),
                        None => (),
                    }
                } else {
                    reply.push_str("No game has been skipped yet")
                }
            }
        }

        self.save_games().unwrap();
        Some(reply)
    }
    fn check_link(&self, game_link: &str) -> bool {
        if let Some(link_start) = &self.config.link_start {
            game_link.starts_with(link_start)
        } else {
            true
        }
    }
    fn submit(&mut self, game_link: String, sender_name: String) -> Option<String> {
        if !self.games_state.is_open {
            Some("The queue is closed. You can not submit your game at the moment.".to_owned())
        } else if !self.config.multiple_submissions
            && (self
                .games_state
                .current_game
                .as_ref()
                .filter(|game| game.author == sender_name)
                .is_some()
                || self
                    .games_state
                    .queue()
                    .any(|game| game.author == sender_name)
                || self
                    .games_state
                    .skipped
                    .iter()
                    .any(|game| game.author == sender_name))
        {
            Some(format!("You can not submit more than one game"))
        } else if self.check_link(&game_link) {
            if let Some(current_game) = &self.games_state.current_game {
                if current_game.name == game_link {
                    return Some(format!(
                        "@{}, we are playing that game right now!",
                        sender_name
                    ));
                }
            }

            if let Some(_) = self.games_state.queue().find(|game| game.name == game_link) {
                return Some(format!(
                    "@{}, that game has already been submitted.",
                    sender_name,
                ));
            }

            if let Some(_) = self
                .games_state
                .skipped
                .iter()
                .find(|game| game.name == game_link)
            {
                return Some(format!(
                    "@{}, your game was skipped. You may return to the queue using !return command",
                    sender_name
                ));
            }

            if let Some(_) = self.played_games.iter().find(|game| game.name == game_link) {
                return Some(format!(
                    "@{}, we have already played that game.",
                    sender_name
                ));
            }

            self.games_state.games_queue.push_back(Game {
                author: sender_name.clone(),
                name: game_link,
            });
            self.save_games().unwrap();
            Some(format!("@{}, your game has been submitted!", sender_name,))
        } else {
            Some(format!("@{}, that link can not be submitted", sender_name))
        }
    }
    fn raffle_start(&mut self) -> Option<String> {
        match &self.games_state.raffle.mode {
            RaffleMode::Active { .. } => Some(format!(
                "The raffle is in progress. Type !join to join the raffle."
            )),
            RaffleMode::Inactive => {
                self.games_state.raffle.mode = RaffleMode::Active {
                    joined: HashMap::new(),
                };
                self.set_current(None);
                self.update_status("The raffle is in progress. Type !join to join the raffle!");
                Some(format!(
                    "The raffle has started! Type !join to join the raffle."
                ))
            }
        }
    }
    fn raffle_finish(&mut self) -> Option<String> {
        let raffle_mode =
            std::mem::replace(&mut self.games_state.raffle.mode, RaffleMode::Inactive);
        let reply = match raffle_mode {
            RaffleMode::Active { joined } => {
                for viewer in joined.keys() {
                    *self
                        .games_state
                        .raffle
                        .viewers_weight
                        .get_mut(viewer)
                        .unwrap() += 1;
                }
                match (joined.into_iter().collect::<Vec<(String, usize)>>())
                    .choose_weighted(&mut rand::thread_rng(), |&(_, weight)| weight)
                {
                    Ok((sender_name, _)) => match self.remove_game(sender_name) {
                        Some(game) => self.set_current(Some(game)),
                        None => {
                            self.games_state.raffle.viewers_weight.remove(sender_name);
                            Some(format!("{} has won the raffle!", sender_name))
                        }
                    },
                    Err(_) => Some(format!("Noone entered the raffle :(")),
                }
            }
            _ => Some(format!("The raffle should be started first: !raffle")),
        };
        self.save_games().unwrap();
        reply
    }
    fn raffle_join(&mut self, sender_name: String) -> Option<String> {
        let weight = *self
            .games_state
            .raffle
            .viewers_weight
            .entry(sender_name.clone())
            .or_insert(self.config.raffle_default_weight);
        match &mut self.games_state.raffle.mode {
            RaffleMode::Active { joined } => {
                if !self
                    .played_games
                    .iter()
                    .any(|game| game.author == sender_name)
                {
                    joined.insert(sender_name, weight);
                }
            }
            _ => (),
        }
        None
    }
    fn raffle_cancel(&mut self) -> Option<String> {
        self.games_state.raffle.mode = RaffleMode::Inactive;
        self.save_games().unwrap();
        Some(format!("Raffle is now inactive"))
    }
    pub fn return_game(&mut self, author_name: &str) -> Option<String> {
        if !self.games_state.is_open {
            return None;
        }
        let reply = if let Some(index) = self
            .games_state
            .skipped
            .iter()
            .enumerate()
            .find(|(_, game)| game.author == author_name)
            .map(|(index, _)| index)
        {
            let game = self.games_state.skipped.remove(index);
            match self.config.return_mode {
                ReturnMode::Front => self.games_state.returned_queue.push_back(game),
                ReturnMode::Back => self.games_state.games_queue.push_back(game),
            }
            self.save_games().unwrap();
            Some(format!(
                "@{}, your game was returned to the queue",
                author_name
            ))
        } else {
            None
        };
        reply
    }
    fn luck(&self, author_name: &str) -> Option<String> {
        self.games_state
            .raffle
            .viewers_weight
            .get(author_name)
            .map(|luck| format!("@{}, your current luck level is {}", author_name, luck))
    }
    pub fn commands() -> BotCommands<Self> {
        BotCommands {
            commands: vec![
                CommandNode::ArgumentNode {
                    argument_type: ArgumentType::Word,
                    child_nodes: vec![CommandNode::FinalNode {
                        authority_level: AuthorityLevel::Any,
                        command: Arc::new(|bot, sender_name, mut args| {
                            let game_link = args.remove(0);
                            if bot.config.allow_direct_link_submit
                                && bot.config.link_start.is_some()
                                && bot.check_link(&game_link)
                            {
                                return bot.submit(game_link, sender_name);
                            }
                            None
                        }),
                    }],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!submit".to_owned()],
                    child_nodes: vec![CommandNode::ArgumentNode {
                        argument_type: ArgumentType::Word,
                        child_nodes: vec![CommandNode::FinalNode {
                            authority_level: AuthorityLevel::Any,
                            command: Arc::new(|bot, sender_name, mut args| {
                                let game_link = args.remove(0);
                                bot.submit(game_link, sender_name)
                            }),
                        }],
                    }],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!return".to_owned()],
                    child_nodes: vec![CommandNode::FinalNode {
                        authority_level: AuthorityLevel::Any,
                        command: Arc::new(|bot, sender_name, _| bot.return_game(&sender_name)),
                    }],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!next".to_owned()],
                    child_nodes: vec![
                        CommandNode::FinalNode {
                            authority_level: AuthorityLevel::Broadcaster,
                            command: Arc::new(|bot, _, _| bot.next(None, true)),
                        },
                        CommandNode::ArgumentNode {
                            argument_type: ArgumentType::Word,
                            child_nodes: vec![CommandNode::FinalNode {
                                authority_level: AuthorityLevel::Broadcaster,
                                command: Arc::new(|bot, _, mut args| {
                                    let author_name = args.remove(0);
                                    bot.next(Some(author_name), false)
                                }),
                            }],
                        },
                    ],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!cancel".to_owned()],
                    child_nodes: vec![
                        CommandNode::FinalNode {
                            authority_level: AuthorityLevel::Any,
                            command: Arc::new(|bot, sender_name, _| {
                                bot.remove_game_response(&sender_name)
                            }),
                        },
                        CommandNode::ArgumentNode {
                            argument_type: ArgumentType::Word,
                            child_nodes: vec![CommandNode::FinalNode {
                                authority_level: AuthorityLevel::Moderator,
                                command: Arc::new(|bot, _, mut args| {
                                    let author_name = args.remove(0);
                                    bot.remove_game_response(&author_name)
                                }),
                            }],
                        },
                    ],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!queue".to_owned(), "!list".to_owned()],
                    child_nodes: vec![CommandNode::FinalNode {
                        authority_level: AuthorityLevel::Any,
                        command: Arc::new(|bot, sender_name, _| {
                            let mut reply = String::new();
                            if bot.config.queue_mode {
                                if let Some((pos, _)) = bot
                                    .games_state
                                    .queue()
                                    .enumerate()
                                    .find(|(_, game)| game.author == sender_name)
                                {
                                    reply.push_str(&format!(
                                        "@{}, your game is {} in the queue. ",
                                        sender_name,
                                        pos + 1
                                    ));
                                }
                            }

                            if bot
                                .games_state
                                .skipped
                                .iter()
                                .any(|game| game.author == sender_name)
                            {
                                reply.push_str(&format!("@{}, your game was skipped. You may return to the queue using !return command. ", sender_name))
                            } else if let Some(game) = &bot.games_state.current_game {
                                if game.author == sender_name {
                                    reply.push_str(&format!(
                                        "@{}, we are currently playing your game. ",
                                        sender_name
                                    ))
                                }
                            }

                            if let Some(config) = &bot.config.google_sheet_config {
                                reply.push_str(&format!("Look at the current queue at: https://docs.google.com/spreadsheets/d/{}/edit#gid=0", config.sheet_id))
                            } else if bot.config.queue_mode {
                                let mut reply = String::new();
                                let games_count = bot.games_state.queue().count();
                                if games_count == 0 {
                                    reply.push_str("The queue is empty");
                                } else {
                                    reply.push_str(&format!(
                                        "There are {} games in the queue",
                                        games_count
                                    ));
                                }
                            }

                            if !reply.is_empty() {
                                Some(reply)
                            } else {
                                None
                            }
                        }),
                    }],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!current".to_owned()],
                    child_nodes: vec![CommandNode::FinalNode {
                        authority_level: AuthorityLevel::Any,
                        command: Arc::new(|bot, _, _| match &bot.games_state.current_game {
                            Some(game) => Some(format!(
                                "Current game is: {} from {}",
                                game.name, game.author
                            )),
                            None => Some("Not playing any game at the moment".to_owned()),
                        }),
                    }],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!skip".to_owned()],
                    child_nodes: vec![
                        CommandNode::LiteralNode {
                            literals: vec!["next".to_owned()],
                            child_nodes: vec![CommandNode::FinalNode {
                                authority_level: AuthorityLevel::Broadcaster,
                                command: Arc::new(|bot, _, _| bot.skip(true)),
                            }],
                        },
                        CommandNode::LiteralNode {
                            literals: vec!["all".to_owned()],
                            child_nodes: vec![CommandNode::FinalNode {
                                authority_level: AuthorityLevel::Broadcaster,
                                command: Arc::new(|bot, _, _| bot.skip_all()),
                            }],
                        },
                        CommandNode::FinalNode {
                            authority_level: AuthorityLevel::Broadcaster,
                            command: Arc::new(|bot, _, _| bot.skip(false)),
                        },
                    ],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!unskip".to_owned()],
                    child_nodes: vec![
                        CommandNode::FinalNode {
                            authority_level: AuthorityLevel::Broadcaster,
                            command: Arc::new(|bot, _, _| bot.unskip(None)),
                        },
                        CommandNode::ArgumentNode {
                            argument_type: ArgumentType::Word,
                            child_nodes: vec![CommandNode::FinalNode {
                                authority_level: AuthorityLevel::Broadcaster,
                                command: Arc::new(|bot, _, mut args| {
                                    let author_name = args.remove(0);
                                    bot.unskip(Some(author_name))
                                }),
                            }],
                        },
                    ],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!stop".to_owned()],
                    child_nodes: vec![CommandNode::FinalNode {
                        authority_level: AuthorityLevel::Broadcaster,
                        command: Arc::new(|bot, _, _| {
                            bot.set_current(None);
                            Some("Current game set to None".to_owned())
                        }),
                    }],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!force".to_owned()],
                    child_nodes: vec![CommandNode::FinalNode {
                        authority_level: AuthorityLevel::Moderator,
                        command: Arc::new(|bot, _, _| {
                            if let Some(_) = bot.time_limit.take() {
                                let game = bot.games_state.current_game.as_ref().unwrap();
                                Some(format!("Now playing {} from @{}", game.name, game.author))
                            } else {
                                Some("Not waiting for response at the moment".to_owned())
                            }
                        }),
                    }],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!close".to_owned()],
                    child_nodes: vec![CommandNode::FinalNode {
                        authority_level: AuthorityLevel::Moderator,
                        command: Arc::new(|bot, _, _| {
                            bot.games_state.is_open = false;
                            bot.save_games().unwrap();
                            Some("The queue is now closed".to_owned())
                        }),
                    }],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!open".to_owned()],
                    child_nodes: vec![CommandNode::FinalNode {
                        authority_level: AuthorityLevel::Moderator,
                        command: Arc::new(|bot, _, _| {
                            bot.games_state.is_open = true;
                            bot.save_games().unwrap();
                            Some("The queue is now open".to_owned())
                        }),
                    }],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!raffle".to_owned()],
                    child_nodes: vec![
                        CommandNode::FinalNode {
                            authority_level: AuthorityLevel::Broadcaster,
                            command: Arc::new(|bot, _, _| bot.raffle_start()),
                        },
                        CommandNode::LiteralNode {
                            literals: vec!["finish".to_owned()],
                            child_nodes: vec![CommandNode::FinalNode {
                                authority_level: AuthorityLevel::Broadcaster,
                                command: Arc::new(|bot, _, _| bot.raffle_finish()),
                            }],
                        },
                        CommandNode::LiteralNode {
                            literals: vec!["cancel".to_owned()],
                            child_nodes: vec![CommandNode::FinalNode {
                                authority_level: AuthorityLevel::Broadcaster,
                                command: Arc::new(|bot, _, _| bot.raffle_cancel()),
                            }],
                        },
                    ],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!join".to_owned()],
                    child_nodes: vec![CommandNode::FinalNode {
                        authority_level: AuthorityLevel::Any,
                        command: Arc::new(|bot, sender_name, _| bot.raffle_join(sender_name)),
                    }],
                },
                CommandNode::LiteralNode {
                    literals: vec!["!luck".to_owned()],
                    child_nodes: vec![CommandNode::FinalNode {
                        authority_level: AuthorityLevel::Any,
                        command: Arc::new(|bot, sender_name, _| bot.luck(&sender_name)),
                    }],
                },
            ],
        }
    }
}
