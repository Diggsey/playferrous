use serde::Deserialize;
use std::{fmt::Write, sync::OnceLock};

const UI_CONFIG: &str = include_str!("ui.toml");

#[derive(Debug, Deserialize)]
pub struct Ui {
    help_text: String,
    group: Vec<UiGroup>,
}

#[derive(Debug, Deserialize)]
struct UiGroup {
    help_text: String,
    #[serde(default)]
    command: Vec<UiCommand>,
}

#[derive(Debug, Deserialize)]
struct UiCommand {
    #[serde(default)]
    help_text: String,
    name: String,
    #[serde(default)]
    args: String,
    #[serde(default)]
    subgroup: Vec<UiGroup>,
}

#[derive(Debug)]
pub enum CommandInterpretation {
    Action { command: String, args: Vec<String> },
    Response { prompt: String },
    Noop,
}

impl Ui {
    pub fn instance() -> &'static Self {
        static INSTANCE: OnceLock<Ui> = OnceLock::new();
        INSTANCE.get_or_init(Self::load)
    }
    fn load() -> Self {
        toml::from_str(UI_CONFIG).expect("UI config to be valid")
    }
    pub fn interpret_command(&self, line: &str) -> anyhow::Result<CommandInterpretation> {
        let res = self.interpret_command_inner(line)?;
        if let CommandInterpretation::Action { command, .. } = &res {
            let mut parts = command
                .split_ascii_whitespace()
                .filter(|part| !part.is_empty());
            if parts.next() == Some("help") {
                return Ok(CommandInterpretation::Response {
                    prompt: self.help(parts)?,
                });
            }
        }
        Ok(res)
    }
    fn help<'a>(&self, mut parts: impl Iterator<Item = &'a str>) -> anyhow::Result<String> {
        Ok(if let Some(part) = parts.next() {
            for group in &self.group {
                for command in &group.command {
                    if command.name.eq_ignore_ascii_case(part) {
                        return command.help(vec![command.name.clone()], parts);
                    }
                }
            }

            format!("Unrecognised command {part}\n")
        } else {
            let mut response = String::new();
            writeln!(response, "{}", self.help_text)?;
            for group in &self.group {
                writeln!(response, "\n{}", group.help_text)?;
                for command in &group.command {
                    writeln!(response, "    {} {}", command.name, command.args)?;
                }
            }

            response
        })
    }
    fn interpret_command_inner(&self, line: &str) -> anyhow::Result<CommandInterpretation> {
        let mut parts = line
            .split_ascii_whitespace()
            .filter(|part| !part.is_empty());
        Ok(if let Some(part) = parts.next() {
            for group in &self.group {
                for command in &group.command {
                    if command.name.eq_ignore_ascii_case(part) {
                        return command.interpret_subcommand(vec![command.name.clone()], parts);
                    }
                }
            }
            CommandInterpretation::Response {
                prompt: "Unrecognised command. Use `help` for more information.\n".into(),
            }
        } else {
            CommandInterpretation::Noop
        })
    }
}

impl UiCommand {
    fn help<'a>(
        &self,
        mut prefix: Vec<String>,
        mut parts: impl Iterator<Item = &'a str>,
    ) -> anyhow::Result<String> {
        let command = prefix.join(" ");
        Ok(if let Some(part) = parts.next() {
            for group in &self.subgroup {
                for command in &group.command {
                    if command.name.eq_ignore_ascii_case(part) {
                        prefix.push(command.name.clone());
                        return command.help(prefix, parts);
                    }
                }
            }

            format!("Unrecognised subcommand {part}\n")
        } else {
            let mut response = String::new();
            writeln!(response, "{} {}\n\n{}", command, self.args, self.help_text)?;
            for group in &self.subgroup {
                writeln!(response, "\n{}", group.help_text)?;
                for command in &group.command {
                    writeln!(response, "    {} {}", command.name, command.args)?;
                }
            }

            response
        })
    }
    fn interpret_subcommand<'a>(
        &self,
        mut prefix: Vec<String>,
        mut parts: impl Iterator<Item = &'a str>,
    ) -> anyhow::Result<CommandInterpretation> {
        let command = prefix.join(" ");
        Ok(if self.subgroup.is_empty() {
            let args: Vec<String> = parts.map(Into::into).collect();
            CommandInterpretation::Action { command, args }
        } else if let Some(part) = parts.next() {
            for group in &self.subgroup {
                for command in &group.command {
                    if command.name.eq_ignore_ascii_case(part) {
                        prefix.push(command.name.clone());
                        return command.interpret_subcommand(prefix, parts);
                    }
                }
            }
            CommandInterpretation::Response {
                prompt: format!(
                    "Unrecognised subcommand. Use `help {command}` for more information.\n"
                ),
            }
        } else {
            CommandInterpretation::Action {
                command,
                args: Vec::new(),
            }
        })
    }
}
