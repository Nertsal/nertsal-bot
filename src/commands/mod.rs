use super::*;

mod command_node;

pub use command_node::*;

pub trait CommandBot<T> {
    fn get_commands(&self) -> &BotCommands<T>;
}

pub async fn check_command<T: CommandBot<T>>(
    bot: &mut T,
    client: &TwitchIRCClient<TCPTransport, StaticLoginCredentials>,
    channel_login: String,
    message: &PrivmsgMessage,
) {
    if let Some((command, args)) = bot.get_commands().find_command(message) {
        if let Some(command_reply) = command(bot, message.sender.name.clone(), args) {
            send_message(client, channel_login, command_reply).await;
        }
    }
}

pub struct BotCommands<T> {
    pub commands: Vec<CommandNode<T>>,
}

pub enum AuthorityLevel {
    Broadcaster,
    Moderator,
    Any,
}

impl<T> BotCommands<T> {
    pub fn find_command(&self, message: &PrivmsgMessage) -> Option<(&Command<T>, Vec<Argument>)> {
        let mut message_text = message.message_text.clone();
        match message_text.remove(0) {
            '!' => self
                .find(message_text.as_str())
                .map(|(command, arguments)| match command {
                    CommandNode::FinalNode {
                        authority_level,
                        command,
                    } => {
                        if check_authority(authority_level, message) {
                            Some((command, arguments))
                        } else {
                            None
                        }
                    }
                    _ => unreachable!(),
                })
                .flatten(),
            _ => None,
        }
    }
    fn find(&self, message: &str) -> Option<(&CommandNode<T>, Vec<Argument>)> {
        self.commands
            .iter()
            .find_map(|com| com.check_node(message, Vec::new()))
    }
}

fn check_authority(authority_level: &AuthorityLevel, message: &PrivmsgMessage) -> bool {
    match authority_level {
        AuthorityLevel::Any => true,
        AuthorityLevel::Broadcaster => check_badges(vec!["broadcaster"], message),
        AuthorityLevel::Moderator => check_badges(vec!["broadcaster", "moderator"], message),
    }
}

fn check_badges(badges: Vec<&str>, message: &PrivmsgMessage) -> bool {
    message
        .badges
        .iter()
        .any(|badge| badges.contains(&badge.name.as_str()))
}
