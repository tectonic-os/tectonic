//! Reads the arguments, runs the command, prints what it produced.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use tect::command::{self, Context, Spec, Verb};
use tect::copy;
use tect::dispatch::{self, Error, USAGE_ERROR};
use tect::model::image::TECT_VERSION;
use tect::prompt::Prompt;

/// Where a person who has run out of commands is sent, which is what an
/// operation that failed says instead of the whole surface.
const COMMANDS: &str = "You can find the available commands by typing 'tect' or 'tect --help'";

/// Whether the banner is already on one of the streams.
static GREETED: AtomicBool = AtomicBool::new(false);

/// The head of what a person reads, once per run. Never on a command whose
/// stdout a script parses, and on stderr when it heads an error.
fn banner(failing: bool) {
    if GREETED.swap(true, Ordering::Relaxed) {
        return;
    }
    match failing {
        true => eprintln!("Tectonic v{TECT_VERSION}\n"),
        false => println!("Tectonic v{TECT_VERSION}\n"),
    }
}

/// The words left after every flag is taken out, and the flags that were there,
/// which is what `only` holds a command to.
struct Args {
    words: Vec<String>,
    given: Vec<&'static str>,
}

impl Args {
    /// Removes every `--<flag> <value>` and `--<flag>=<value>`.
    fn flags(&mut self, flag: &'static str) -> Result<Vec<String>, Error> {
        let mut values = Vec::new();
        let mut i = 0;
        while i < self.words.len() {
            let taken = if let Some(v) = self.words[i].strip_prefix(&format!("--{flag}=")) {
                values.push(v.to_string());
                1
            } else if self.words[i] == format!("--{flag}") {
                values.push(
                    self.words
                        .get(i + 1)
                        .ok_or_else(|| Error::Invocation(format!("`--{flag}` takes a value")))?
                        .clone(),
                );
                2
            } else {
                i += 1;
                continue;
            };
            self.words.drain(i..i + taken);
        }
        if !values.is_empty() {
            self.given.push(flag);
        }
        Ok(values)
    }

    fn flag(&mut self, flag: &'static str) -> Result<Option<String>, Error> {
        Ok(self.flags(flag)?.pop())
    }

    /// Removes `--<flag>`. Not recorded: a switch belongs to every command.
    fn switch(&mut self, flag: &str) -> bool {
        let before = self.words.len();
        self.words.retain(|arg| arg != &format!("--{flag}"));
        self.words.len() != before
    }

    /// A flag the command does not read is a failure rather than a silent no-op.
    fn only(&self, spec: &Spec) -> Result<(), Error> {
        match self.given.iter().find(|flag| !spec.takes.contains(flag)) {
            Some(flag) => Err(Error::Invocation(format!(
                "`{}` does not take `--{flag}`",
                spec.name()
            ))),
            None => Ok(()),
        }
    }
}

fn main() -> ExitCode {
    // Rust ignores SIGPIPE, so `tect plan | head` panics on the write rather
    // than ending the run. Every print here is a person's or a script's.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };

    match run() {
        Ok(code) => code,
        Err(error) => {
            banner(true);
            let unknown = matches!(error, Error::Usage(_));
            let message = error.message();
            // A message that is already a sentence, or a block of them, keeps
            // its own punctuation.
            let stop = match message.ends_with(['.', '!', '?']) || message.contains('\n') {
                true => "",
                false => ".",
            };
            eprintln!("Error: {message}{stop}\n");
            match unknown {
                // The words were not a command, so nothing has resolved a
                // place yet; the surface is still what it is where this ran.
                true => eprint!("{}", command::usage(&Context::of(None))),
                false => eprintln!("{COMMANDS}"),
            }
            ExitCode::from(USAGE_ERROR)
        }
    }
}

/// Whether the words are a list rather than a command: nothing typed at all,
/// or a verb alone where every form of it takes a noun. `scap` and `fetch` are
/// neither, since a picker of their nouns would hide the half that takes an
/// argument instead.
fn picking(words: &[&str], prompt: &Prompt, here: &Context) -> bool {
    prompt.draws()
        && match words {
            [] => true,
            // A picker with nothing in it teaches less than the refusal
            // naming what the verb takes.
            [word] => {
                command::all_nouns(word)
                    && command::nouns(word).iter().any(|spec| spec.runs_in(here))
            }
            _ => false,
        }
}

fn run() -> Result<ExitCode, Error> {
    let mut args = Args {
        words: std::env::args().skip(1).collect(),
        given: Vec::new(),
    };
    if args.words == ["--version"] {
        println!("Tectonic v{TECT_VERSION}");
        return Ok(ExitCode::SUCCESS);
    }
    let prompt = Prompt::new(args.switch("no-tui"));
    let cache_to = args.switch("cache-to");
    let no_cache_from = args.switch("no-cache-from");
    let rebuild = args.switch("rebuild");
    let flags = dispatch::Flags {
        root: args.flag("root")?.map(PathBuf::from),
        owner: args.flag("owner")?,
        host: args.flag("host")?,
        images: args.flags("image")?,
        module: args.flag("module")?,
        cn: args.flag("cn")?,
        from: args.flag("from")?,
        disk: args.flag("disk")?,
        user: args.flag("user")?,
        password: args.flag("password")?,
        base: args.flag("base")?,
        format: args.flag("format")?,
        target: args.flag("target")?,
        datastream: args.flag("datastream")?.map(PathBuf::from),
        baseline: args.flag("baseline")?.map(PathBuf::from),
        base_scan: args.flag("base-scan")?.map(PathBuf::from),
        tags: args.flags("tag")?,
        kernel: args.flag("kernel")?,
        ram: args.flag("ram")?,
        backend: args.flag("backend")?,
        oci_output: args.flag("oci-output")?,
        secrets: args.flags("secret")?,
        pkgs: args.flags("pkg")?,
        with: args
            .flags("with")?
            .iter()
            .map(|pair| match pair.split_once('=') {
                Some((verb, value)) => Ok((verb.to_string(), value.to_string())),
                None => Err(Error::Invocation(format!(
                    "`--with` is `verb=value`, not `{pair}`"
                ))),
            })
            .collect::<Result<Vec<_>, Error>>()?,
        cache_to,
        no_cache_from,
        rebuild,
    };

    // Where this is running, asked once and passed to everything that renders
    // the surface or opens a repository.
    let here = Context::of(flags.root.as_deref());

    let words: Vec<&str> = args.words.iter().map(String::as_str).collect();
    let (spec, rest): (&Spec, &[&str]) = if matches!(words.first(), Some(&"-h") | Some(&"--help")) {
        banner(false);
        print!("{}", command::usage(&here));
        return Ok(ExitCode::SUCCESS);
    } else if picking(&words, &prompt, &here) {
        let listed = match words.first() {
            Some(word) => command::nouns(word),
            None => command::listed(),
        };
        let (rows, options) = command::choices(&listed, &here);
        banner(false);
        match tect::ui::select(copy::WHICH_COMMAND, &options)? {
            Some(at) => (rows[at], &[]),
            None => return Ok(ExitCode::SUCCESS),
        }
    } else if words.is_empty() {
        banner(true);
        eprint!("{}", command::usage(&here));
        return Ok(ExitCode::from(USAGE_ERROR));
    } else {
        command::resolve(&words).map_err(Error::Usage)?
    };

    args.only(spec)?;
    if matches!(
        spec.verb,
        Verb::CreateRepo
            | Verb::CreateImage
            | Verb::CreateFlavour
            | Verb::CreateModule
            | Verb::CreateKey
            | Verb::SetKey
            | Verb::ImportModule
            | Verb::CopyModule
            | Verb::Check
    ) {
        banner(false);
    }

    dispatch::dispatch(spec, rest, flags, &prompt, &here)
}
