//! What no flag gave. Every command takes flags for everything it needs; this
//! fills what they left empty when there is someone to ask, and names the flag
//! when there is not.

use std::io::{IsTerminal, Write};

pub struct Prompt {
    ask: bool,
}

impl Prompt {
    pub fn new(no_tui: bool) -> Self {
        Self {
            ask: !no_tui && std::io::stdin().is_terminal(),
        }
    }

    /// Nobody to ask: every missing value fails naming its flag.
    pub fn silent() -> Self {
        Self { ask: false }
    }

    pub fn asks(&self) -> bool {
        self.ask
    }

    fn read(&self, question: &str) -> Result<String, String> {
        print!("{question}: ");
        std::io::stdout().flush().map_err(|err| err.to_string())?;
        let mut answer = String::new();
        std::io::stdin()
            .read_line(&mut answer)
            .map_err(|err| err.to_string())?;
        Ok(answer.trim().to_string())
    }

    /// What the flag gave, else the answer, else the default, else a failure
    /// naming `flag`.
    pub fn text(
        &self,
        given: Option<String>,
        question: &str,
        flag: &str,
        default: Option<&str>,
    ) -> Result<String, String> {
        if let Some(value) = given.filter(|value| !value.is_empty()) {
            return Ok(value);
        }
        let missing = || match default {
            Some(default) => Ok(default.to_string()),
            None => Err(format!("no {flag} and no terminal to ask on: {question}")),
        };
        if !self.ask {
            return missing();
        }
        let answer = match default {
            Some(default) => self.read(&format!("{question} [{default}]"))?,
            None => self.read(question)?,
        };
        match answer.is_empty() {
            true => missing(),
            false => Ok(answer),
        }
    }

    /// A step with no flag of its own: the flag that answers it is the answer,
    /// so with nobody to ask the answer is no.
    pub fn confirm(&self, question: &str) -> Result<bool, String> {
        if !self.ask {
            return Ok(false);
        }
        let answer = self.read(&format!("{question} [Y/n]"))?;
        Ok(!answer.starts_with(['n', 'N']))
    }

    /// One of a set, by number or by name, or none of them.
    pub fn choose(&self, question: &str, options: &[String]) -> Result<Option<usize>, String> {
        if !self.ask || options.is_empty() {
            return Ok(None);
        }
        for (index, option) in options.iter().enumerate() {
            println!("  {}) {option}", index + 1);
        }
        println!("  0) none");
        let answer = self.read(&format!("{question} [0-{}]", options.len()))?;
        match answer.parse::<usize>() {
            Ok(0) => Ok(None),
            Ok(number) if number <= options.len() => Ok(Some(number - 1)),
            _ => options
                .iter()
                .position(|option| *option == answer)
                .map(Some)
                .ok_or_else(|| format!("`{answer}` is not one of them")),
        }
    }
}
