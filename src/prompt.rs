//! What no flag gave. Every command takes flags for everything it needs; this
//! fills what they left empty when there is someone to ask, and names the flag
//! when there is not.

use crate::ui::Choice;
use std::cell::Cell;
use std::io::{IsTerminal, Write};

/// A file of answers, one per line, which a run answers from instead of asking.
/// What the transcript goldens drive the binary with.
const SCRIPT: &str = "TECT_ANSWERS";

pub struct Prompt {
    ask: bool,
    /// Whether an answer may be asked for with a widget rather than plain
    /// lines, which redirected output rules out.
    draw: bool,
    /// The answers a scripted run reads, and how far through them it is.
    script: Option<(Vec<String>, Cell<usize>)>,
}

impl Prompt {
    pub fn new(no_tui: bool) -> Self {
        if let Some(path) = std::env::var_os(SCRIPT) {
            let text = std::fs::read_to_string(path).unwrap_or_default();
            return Self::scripted(text.lines().map(str::to_string).collect());
        }
        let ask = !no_tui && std::io::stdin().is_terminal();
        Self {
            ask,
            draw: ask && std::io::stdout().is_terminal(),
            script: None,
        }
    }

    /// Nobody to ask: every missing value fails naming its flag.
    pub fn silent() -> Self {
        Self {
            ask: false,
            draw: false,
            script: None,
        }
    }

    /// The answers in order, whatever the terminal is. A question they run out
    /// for fails, so an unexpected one is a failure rather than a wait.
    pub fn scripted(answers: Vec<String>) -> Self {
        Self {
            ask: true,
            draw: false,
            script: Some((answers, Cell::new(0))),
        }
    }

    /// Whether there is anyone to ask, which is what a line standing with a
    /// question rather than before one has to know.
    pub fn asks(&self) -> bool {
        self.ask
    }

    /// Whether an answer may be drawn, which is what a command with nothing to
    /// do but ask has to know before it opens a picker instead of refusing.
    pub fn draws(&self) -> bool {
        self.draw
    }

    /// `shown` is everything that stands before the answer, and `question` is
    /// what a run with no answer left for it names. One blank line follows,
    /// which is what separates a question from what comes after it.
    fn read(&self, question: &str, shown: &str) -> Result<String, String> {
        let answer = self.answer(question, shown)?;
        println!();
        Ok(answer)
    }

    fn answer(&self, question: &str, shown: &str) -> Result<String, String> {
        if let Some((answers, at)) = &self.script {
            let answer = answers
                .get(at.get())
                .ok_or_else(|| format!("nothing left in {SCRIPT} to answer `{question}` with"))?;
            at.set(at.get() + 1);
            println!("{shown}{answer}");
            return Ok(answer.trim().to_string());
        }
        print!("{shown}");
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
            None => Err(format!(
                "give {flag}, since nothing can be asked here: {question}"
            )),
        };
        if !self.ask {
            return missing();
        }
        let answer = match default {
            Some(default) => {
                let question = format!("{question} [{default}]");
                self.read(&question, &format!("{question}: "))?
            }
            None => self.read(question, &format!("{question}: "))?,
        };
        match answer.is_empty() {
            true => missing(),
            false => Ok(answer),
        }
    }

    /// The same, asked over two lines: the question on its own, the answer
    /// typed after `prefix`, so what the answer belongs to stays visible.
    pub fn line(
        &self,
        given: Option<String>,
        question: &str,
        flag: &str,
        prefix: &str,
        default: Option<&str>,
    ) -> Result<String, String> {
        if let Some(value) = given.filter(|value| !value.is_empty()) {
            return Ok(value);
        }
        let missing = || match default {
            Some(default) => Ok(default.to_string()),
            None => Err(format!(
                "give {flag}, since nothing can be asked here: {question}"
            )),
        };
        if !self.ask {
            return missing();
        }
        let question = match default {
            Some(default) => format!("{question} [{default}]"),
            None => question.to_string(),
        };
        match self.read(&question, &format!("{question}\n{prefix}"))? {
            answer if answer.is_empty() => missing(),
            answer => Ok(answer),
        }
    }

    /// A step with no flag of its own: the flag that answers it is the answer,
    /// so with nobody to ask the answer is no. `yes` and `no` are what the two
    /// answers are called, since not every one of them is a refusal.
    pub fn confirm(&self, question: &str, yes: &str, no: &str) -> Result<bool, String> {
        if !self.ask {
            return Ok(false);
        }
        if self.draw {
            let chosen = crate::ui::confirm(question, yes, no)?;
            println!("{question}: {}\n", if chosen { yes } else { no });
            return Ok(chosen);
        }
        let question = format!("{question} ({yes}/{no})");
        let answer = self.read(&question, &format!("{question}\n"))?;
        let first = |word: &str| word.chars().next().map(|c| c.to_ascii_lowercase());
        Ok(first(&answer) != first(no))
    }

    /// One of a set, or none of them: an inline select list where the output is
    /// a terminal, a numbered list to answer by number or by name where it is
    /// not.
    pub fn choose(&self, question: &str, options: &[Choice]) -> Result<Option<usize>, String> {
        if !self.ask || options.is_empty() {
            return Ok(None);
        }
        if self.draw {
            let chosen = crate::ui::select(question, options)?;
            let answer = match chosen {
                Some(index) => options[index].label.as_str(),
                None => "none",
            };
            println!("{question}: {answer}\n");
            return Ok(chosen);
        }
        self.numbered(options);
        let question = format!("{question} [{}, 0 for none]", range(options));
        let answer = self.read(&question, &format!("{question}: "))?;
        match answer.parse::<usize>() {
            Ok(0) => Ok(None),
            Ok(number) if number <= options.len() => Ok(Some(number - 1)),
            _ => options
                .iter()
                .position(|option| option.label == answer)
                .map(Some)
                .ok_or_else(|| format!("`{answer}` is not one of them")),
        }
    }

    /// Any of a set, in the order they were answered, or none of them: an
    /// inline multi-select where the output is a terminal, the numbered list
    /// taking several numbers or names on the one line where it is not.
    pub fn choose_many(&self, question: &str, options: &[Choice]) -> Result<Vec<usize>, String> {
        if !self.ask || options.is_empty() {
            return Ok(Vec::new());
        }
        if self.draw {
            let chosen = match crate::ui::multi(question, options)? {
                crate::ui::Answer::Cancelled => Vec::new(),
                crate::ui::Answer::Chosen(chosen) => chosen,
            };
            let answer = match chosen.is_empty() {
                true => "none".to_string(),
                false => chosen
                    .iter()
                    .map(|at| options[*at].label.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            };
            println!("{question}: {answer}\n");
            return Ok(chosen);
        }
        self.numbered(options);
        let several = match options.len() {
            1 => "",
            _ => ", several",
        };
        let question = format!("{question} [{}{several}, 0 for none]", range(options));
        let answer = self.read(&question, &format!("{question}: "))?;

        let mut chosen: Vec<usize> = Vec::new();
        for word in answer.split([' ', ',']).filter(|word| !word.is_empty()) {
            let at = match word.parse::<usize>() {
                Ok(0) => return Ok(Vec::new()),
                Ok(number) if number <= options.len() => number - 1,
                _ => options
                    .iter()
                    .position(|option| option.label == word)
                    .ok_or_else(|| format!("`{word}` is not one of them"))?,
            };
            if !chosen.contains(&at) {
                chosen.push(at);
            }
        }
        Ok(chosen)
    }

    fn numbered(&self, options: &[Choice]) {
        for (index, option) in options.iter().enumerate() {
            let line = format!("{}  {}", option.label, option.detail);
            println!("  {}) {}", index + 1, line.trim_end());
        }
    }
}

/// What answers by number the question takes, which is one number when there is
/// one option rather than a range of one.
fn range(options: &[Choice]) -> String {
    match options.len() {
        1 => "1".to_string(),
        n => format!("1-{n}"),
    }
}
