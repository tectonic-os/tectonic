//! Diagnostics with source spans.

use miette::{Diagnostic, LabeledSpan, NamedSource, SourceSpan};
use std::fmt;
use std::sync::Arc;

/// One file, read once and shared by every diagnostic that points into it, so
/// a diagnostic names its source rather than carrying a copy of it.
#[derive(Clone)]
pub struct Source(Arc<NamedSource<String>>);

impl Source {
    pub fn new(file: impl AsRef<str>, text: impl Into<String>) -> Self {
        Self(Arc::new(
            NamedSource::new(file, text.into()).with_language("KDL"),
        ))
    }

    /// The path it was read from, which is what the plan and a help line print.
    pub fn name(&self) -> &str {
        self.0.name()
    }
}

/// A byte range in a source file: what the model carries, rather than the
/// parser's own span type.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Span {
    pub offset: usize,
    pub len: usize,
}

impl From<SourceSpan> for Span {
    fn from(span: SourceSpan) -> Self {
        Self {
            offset: span.offset(),
            len: span.len(),
        }
    }
}

impl From<Span> for SourceSpan {
    fn from(span: Span) -> Self {
        (span.offset, span.len).into()
    }
}

/// One problem, with the source it was found in and the spans to underline.
pub struct Issue {
    message: String,
    src: Source,
    labels: Vec<LabeledSpan>,
    help: Option<String>,
}

impl Issue {
    pub fn new(message: impl Into<String>, src: &Source) -> Self {
        Self {
            message: message.into(),
            src: src.clone(),
            labels: Vec::new(),
            help: None,
        }
    }

    pub fn at(mut self, span: impl Into<Span>, label: impl Into<String>) -> Self {
        let span = SourceSpan::from(span.into());
        self.labels
            .push(LabeledSpan::new_with_span(Some(label.into()), span));
        self
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl fmt::Debug for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Issue {}

impl Diagnostic for Issue {
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&*self.src.0)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        if self.labels.is_empty() {
            None
        } else {
            Some(Box::new(self.labels.iter().cloned()))
        }
    }

    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        self.help
            .as_ref()
            .map(|h| Box::new(h) as Box<dyn fmt::Display>)
    }
}

/// Collects every problem before reporting, so one run surfaces all of them.
#[derive(Default)]
pub struct Issues(Vec<Issue>);

impl Issues {
    pub fn push(&mut self, issue: Issue) {
        self.0.push(issue);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every issue rendered without colour or hyperlinks, at a fixed width, so
    /// the same repository produces the same bytes on any terminal.
    pub fn plain(&self) -> String {
        let handler = miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::none())
            .with_width(80);
        let mut out = String::new();
        for issue in &self.0 {
            let _ = handler.render_report(&mut out, issue);
            out.push('\n');
        }
        out
    }

    /// Prints every issue and returns whether any were found.
    pub fn report(self, context: &str) -> bool {
        let found = !self.0.is_empty();
        let count = self.0.len();
        for issue in self.0 {
            eprintln!("{:?}", miette::Report::new(issue));
        }
        if found {
            eprintln!(
                "tect: {count} problem{} in {context}",
                if count == 1 { "" } else { "s" }
            );
        }
        found
    }
}
