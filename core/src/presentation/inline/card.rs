//! A plan or a summary as one card.

use ratatui::text::{Line, Span};

use super::style::{self, Palette};
use super::text;
use crate::presentation::tui::Tone;

/// What a change does to its subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Add,
    Change,
    Remove,
}

impl Mark {
    const fn glyph(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Change => "~",
            Self::Remove => "-",
        }
    }

    const fn tone(self) -> Tone {
        match self {
            Self::Add => Tone::Ok,
            Self::Change => Tone::Warn,
            Self::Remove => Tone::Bad,
        }
    }
}

/// A cell of a table or the value of a field: text and how it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    pub text: String,
    pub tone: Option<Tone>,
    pub dim: bool,
}

impl Value {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: None,
            dim: false,
        }
    }

    #[must_use]
    pub fn dim(text: impl Into<String>) -> Self {
        Self {
            dim: true,
            ..Self::new(text)
        }
    }

    #[must_use]
    pub fn tone(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            tone: Some(tone),
            ..Self::new(text)
        }
    }

    fn style(&self, palette: Palette) -> ratatui::style::Style {
        match (self.tone, self.dim) {
            (Some(tone), _) => palette.tone(tone),
            (None, true) => palette.dim(),
            (None, false) => palette.text(),
        }
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Block {
    Fields(Vec<(String, Vec<Value>)>),
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<Value>>,
    },
    Changes {
        changed: Vec<(Mark, String, String)>,
        unchanged: Option<String>,
    },
    Notes(Vec<(Tone, String)>),
    Command {
        heading: String,
        command: String,
    },
}

/// A card, built block by block and drawn at any width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    title: String,
    badge: Option<String>,
    blocks: Vec<Block>,
}

impl Card {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            badge: None,
            blocks: Vec::new(),
        }
    }

    /// Short text at the right of the title line, such as `plan 3f9a1c2e7b4d`.
    #[must_use]
    pub fn badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    /// A key/value row; rows that follow each other align their values.
    pub fn field(&mut self, key: &str, value: impl Into<Value>) -> &mut Self {
        self.fields(key, vec![value.into()])
    }

    /// A key/value row whose value has parts that read differently.
    pub fn fields(&mut self, key: &str, value: Vec<Value>) -> &mut Self {
        if let Some(Block::Fields(fields)) = self.blocks.last_mut() {
            fields.push((key.to_owned(), value));
        } else {
            self.blocks
                .push(Block::Fields(vec![(key.to_owned(), value)]));
        }
        self
    }

    /// A table with these column headings.
    pub fn table(&mut self, columns: &[&str], rows: Vec<Vec<Value>>) -> &mut Self {
        if !rows.is_empty() {
            self.blocks.push(Block::Table {
                columns: columns.iter().map(|column| (*column).to_owned()).collect(),
                rows,
            });
        }
        self
    }

    /// What changes, and one folded line for what does not.
    pub fn changes(
        &mut self,
        changed: Vec<(Mark, String, String)>,
        unchanged: Option<String>,
    ) -> &mut Self {
        if !changed.is_empty() || unchanged.is_some() {
            self.blocks.push(Block::Changes { changed, unchanged });
        }
        self
    }

    /// Notes, each marked with its tone.
    pub fn notes(&mut self, notes: Vec<(Tone, String)>) -> &mut Self {
        if !notes.is_empty() {
            self.blocks.push(Block::Notes(notes));
        }
        self
    }

    /// The command to run next, under a short heading.
    pub fn command(&mut self, heading: &str, command: impl Into<String>) -> &mut Self {
        self.blocks.push(Block::Command {
            heading: heading.to_owned(),
            command: command.into(),
        });
        self
    }

    /// The card's lines, `width` columns wide.
    #[must_use]
    pub fn lines(&self, palette: Palette, width: usize) -> Vec<Line<'static>> {
        let mut lines = vec![text::row(
            vec![Span::styled(self.title.clone(), palette.strong())],
            self.badge
                .iter()
                .map(|badge| Span::styled(badge.clone(), palette.dim()))
                .collect(),
            width,
        )];
        for (index, block) in self.blocks.iter().enumerate() {
            if index > 0 || !matches!(block, Block::Fields(_)) {
                lines.push(Line::default());
            }
            match block {
                Block::Fields(fields) => lines.extend(field_lines(fields, palette, width)),
                Block::Table { columns, rows } => {
                    lines.extend(table_lines(columns, rows, palette, width));
                }
                Block::Changes { changed, unchanged } => {
                    lines.extend(change_lines(changed, unchanged.as_deref(), palette, width));
                }
                Block::Notes(notes) => lines.extend(note_lines(notes, palette, width)),
                Block::Command { heading, command } => {
                    lines.push(Line::styled(heading.clone(), palette.text()));
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            crate::presentation::output::terminal_safe(command),
                            palette.accent(),
                        ),
                    ]));
                }
            }
        }
        lines
    }
}

fn field_lines(
    fields: &[(String, Vec<Value>)],
    palette: Palette,
    width: usize,
) -> Vec<Line<'static>> {
    let column = fields
        .iter()
        .map(|(key, _)| text::width(key))
        .max()
        .unwrap_or(0);
    fields
        .iter()
        .map(|(key, value)| {
            let mut spans = vec![
                Span::raw("  "),
                Span::styled(text::pad(key, column), palette.dim()),
                Span::raw("  "),
            ];
            for (index, part) in value.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::styled(" · ", palette.dim()));
                }
                spans.push(Span::styled(part.text.clone(), part.style(palette)));
            }
            Line::from(text::fit(spans, width))
        })
        .collect()
}

/// Columns as wide as their widest cell.
fn table_lines(
    columns: &[String],
    rows: &[Vec<Value>],
    palette: Palette,
    width: usize,
) -> Vec<Line<'static>> {
    let mut widths = columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            rows.iter()
                .filter_map(|row| row.get(index))
                .map(|cell| text::width(&cell.text))
                .chain([text::width(column)])
                .max()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    let gaps = 2 + 2 * columns.len().saturating_sub(1);
    while widths.iter().sum::<usize>() + gaps > width {
        let Some(widest) = widths.iter_mut().max() else {
            break;
        };
        if *widest <= 6 {
            break;
        }
        *widest -= 1;
    }
    let line = |cells: Vec<Span<'static>>| {
        let mut spans = vec![Span::raw("  ")];
        let last = cells.len().saturating_sub(1);
        for (index, cell) in cells.into_iter().enumerate() {
            let column = widths.get(index).copied().unwrap_or(0);
            let content = if index == last {
                text::clip(&cell.content, column)
            } else {
                text::pad(&cell.content, column)
            };
            spans.push(Span::styled(content, cell.style));
            if index < last {
                spans.push(Span::raw("  "));
            }
        }
        Line::from(text::fit(spans, width))
    };
    let mut lines = vec![line(
        columns
            .iter()
            .map(|column| Span::styled(column.clone(), palette.dim()))
            .collect(),
    )];
    for row in rows {
        lines.push(line(
            row.iter()
                .map(|cell| Span::styled(cell.text.clone(), cell.style(palette)))
                .collect(),
        ));
    }
    lines
}

fn change_lines(
    changed: &[(Mark, String, String)],
    unchanged: Option<&str>,
    palette: Palette,
    width: usize,
) -> Vec<Line<'static>> {
    let column = changed
        .iter()
        .map(|(_, subject, _)| text::width(subject))
        .max()
        .unwrap_or(0)
        .min(24);
    let mut lines = vec![Line::styled("Changes", palette.text())];
    let indent = 6 + column;
    for (mark, subject, change) in changed {
        let room = width.saturating_sub(indent).max(20);
        for (index, part) in wrap(&crate::presentation::output::terminal_safe(change), room)
            .into_iter()
            .enumerate()
        {
            let lead = if index == 0 {
                vec![
                    Span::raw("  "),
                    Span::styled(mark.glyph(), palette.tone(mark.tone())),
                    Span::raw(" "),
                    Span::styled(text::pad(subject, column), palette.text()),
                    Span::raw("  "),
                ]
            } else {
                vec![Span::raw(" ".repeat(indent))]
            };
            let mut spans = lead;
            spans.push(Span::styled(part, palette.text()));
            lines.push(Line::from(spans));
        }
    }
    if let Some(unchanged) = unchanged {
        lines.push(Line::from(text::fit(
            vec![
                Span::raw("  "),
                Span::styled(style::WAITING, palette.dim()),
                Span::raw(" "),
                Span::styled(unchanged.to_owned(), palette.dim()),
            ],
            width,
        )));
    }
    lines
}

fn note_lines(notes: &[(Tone, String)], palette: Palette, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (tone, note) in notes {
        let room = width.saturating_sub(4).max(20);
        for (index, part) in wrap(&crate::presentation::output::terminal_safe(note), room)
            .into_iter()
            .enumerate()
        {
            let lead = if index == 0 {
                Span::styled(format!("  {} ", tone.glyph()), palette.tone(*tone))
            } else {
                Span::raw("    ")
            };
            lines.push(Line::from(vec![lead, Span::styled(part, palette.text())]));
        }
    }
    lines
}

/// `text` broken at spaces into lines of at most `width` columns.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split(' ') {
        let needed = if current.is_empty() {
            text::width(word)
        } else {
            text::width(&current) + 1 + text::width(word)
        };
        if needed > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
        if text::width(&current) > width {
            lines.push(text::clip(&current, width));
            current.clear();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}
