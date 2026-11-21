//! Cohesive TrueColor themes with a no-color fallback.

use std::fmt::Write as _;

/// One RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Returns the `38;2` foreground sequence body.
    #[must_use]
    pub fn fg_sequence(self) -> String {
        format!("38;2;{};{};{}", self.0, self.1, self.2)
    }

    /// Returns the `48;2` background sequence body.
    #[must_use]
    pub fn bg_sequence(self) -> String {
        format!("48;2;{};{};{}", self.0, self.1, self.2)
    }
}

/// Terminal style. In monochrome mode colors are ignored and only attributes
/// are emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)] // Terminal attributes are independent flags.
pub struct Style {
    /// Foreground color.
    pub fg: Option<Rgb>,
    /// Background color.
    pub bg: Option<Rgb>,
    /// Bold attribute.
    pub bold: bool,
    /// Dim attribute.
    pub dim: bool,
    /// Italic attribute.
    pub italic: bool,
    /// Reverse-video attribute.
    pub reverse: bool,
}

impl Style {
    /// Foreground-only style.
    #[must_use]
    pub fn fg(color: Rgb) -> Self {
        Self {
            fg: Some(color),
            ..Self::default()
        }
    }

    /// Foreground plus background style.
    #[must_use]
    pub fn on(color: Rgb, background: Rgb) -> Self {
        Self {
            fg: Some(color),
            bg: Some(background),
            ..Self::default()
        }
    }

    /// Returns this style in bold.
    #[must_use]
    pub fn bold(self) -> Self {
        Self { bold: true, ..self }
    }

    /// Returns this style in dim.
    #[must_use]
    pub fn dim(self) -> Self {
        Self { dim: true, ..self }
    }

    /// Returns this style in italic.
    #[must_use]
    pub fn italic(self) -> Self {
        Self {
            italic: true,
            ..self
        }
    }

    /// Returns this style reversed.
    #[must_use]
    pub fn reverse(self) -> Self {
        Self {
            reverse: true,
            ..self
        }
    }
}

/// Built-in theme identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    /// Deep slate truecolor.
    Midnight,
    /// Black background with vivid foregrounds.
    HighContrast,
    /// No color escape sequences at all.
    Monochrome,
    /// Soft neutral paper-gray with deep readable ink.
    Light,
}

impl ThemeId {
    /// Cycles Midnight -> HighContrast -> Monochrome -> Light -> Midnight.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Midnight => Self::HighContrast,
            Self::HighContrast => Self::Monochrome,
            Self::Monochrome => Self::Light,
            Self::Light => Self::Midnight,
        }
    }

    /// Display name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Midnight => "Midnight",
            Self::HighContrast => "High-Contrast",
            Self::Monochrome => "Monochrome",
            Self::Light => "Light",
        }
    }
}

/// Active palette.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Theme identity.
    pub id: ThemeId,
    /// Full-screen canvas background.
    pub canvas_bg: Rgb,
    /// Panel background.
    pub panel_bg: Rgb,
    /// Header and status background.
    pub header_bg: Rgb,
    /// Active cursor-line background.
    pub active_line_bg: Rgb,
    /// Error-line tint.
    pub error_bg: Rgb,
    /// Focused border accent.
    pub focus_border: Rgb,
    /// Unfocused border.
    pub unfocused_border: Rgb,
    /// Primary text.
    pub text: Rgb,
    /// Muted text and line numbers.
    pub muted: Rgb,
    /// Active line number.
    pub active_line_number: Rgb,
    /// Control anchors.
    pub control: Rgb,
    /// Lexicon computational verbs.
    pub action_verb: Rgb,
    /// ACE anaphors and quantifiers.
    pub anaphor: Rgb,
    /// Hardware types.
    pub type_color: Rgb,
    /// Numeric literals.
    pub number: Rgb,
    /// String and byte literals.
    pub string: Rgb,
    /// Operators and punctuation.
    pub operator: Rgb,
    /// Omitted prose.
    pub omitted: Rgb,
    /// Success green.
    pub success: Rgb,
    /// Warning amber.
    pub warning: Rgb,
    /// Error crimson.
    pub error: Rgb,
}

impl Theme {
    /// Returns the palette for an identity.
    #[must_use]
    pub fn from_id(id: ThemeId) -> Self {
        match id {
            ThemeId::Midnight => Self {
                id,
                canvas_bg: Rgb(22, 25, 34),
                panel_bg: Rgb(28, 32, 43),
                header_bg: Rgb(18, 21, 30),
                active_line_bg: Rgb(36, 42, 58),
                error_bg: Rgb(58, 28, 36),
                focus_border: Rgb(122, 162, 247),
                unfocused_border: Rgb(56, 64, 86),
                text: Rgb(216, 222, 238),
                muted: Rgb(92, 102, 134),
                active_line_number: Rgb(224, 175, 104),
                control: Rgb(187, 154, 247),
                action_verb: Rgb(125, 207, 255),
                anaphor: Rgb(42, 195, 222),
                type_color: Rgb(224, 175, 104),
                number: Rgb(255, 158, 100),
                string: Rgb(158, 206, 106),
                operator: Rgb(137, 221, 255),
                omitted: Rgb(108, 117, 144),
                success: Rgb(115, 218, 202),
                warning: Rgb(224, 175, 104),
                error: Rgb(247, 118, 142),
            },
            ThemeId::HighContrast => Self {
                id,
                canvas_bg: Rgb(0, 0, 0),
                panel_bg: Rgb(0, 0, 0),
                header_bg: Rgb(0, 0, 0),
                active_line_bg: Rgb(34, 34, 34),
                error_bg: Rgb(70, 0, 8),
                focus_border: Rgb(0, 224, 255),
                unfocused_border: Rgb(150, 150, 150),
                text: Rgb(255, 255, 255),
                muted: Rgb(180, 180, 180),
                active_line_number: Rgb(255, 255, 0),
                control: Rgb(255, 96, 255),
                action_verb: Rgb(0, 255, 255),
                anaphor: Rgb(80, 255, 120),
                type_color: Rgb(255, 255, 0),
                number: Rgb(255, 160, 0),
                string: Rgb(120, 255, 120),
                operator: Rgb(0, 224, 255),
                omitted: Rgb(170, 170, 170),
                success: Rgb(0, 255, 80),
                warning: Rgb(255, 220, 0),
                error: Rgb(255, 40, 40),
            },
            ThemeId::Light => Self {
                id,
                canvas_bg: Rgb(246, 248, 250),
                panel_bg: Rgb(255, 255, 255),
                header_bg: Rgb(240, 243, 246),
                active_line_bg: Rgb(230, 240, 255),
                error_bg: Rgb(255, 235, 233),
                focus_border: Rgb(9, 105, 218),
                unfocused_border: Rgb(208, 215, 222),
                text: Rgb(31, 35, 40),
                muted: Rgb(87, 96, 106),
                active_line_number: Rgb(140, 149, 159),
                control: Rgb(5, 80, 174),
                action_verb: Rgb(9, 105, 218),
                anaphor: Rgb(14, 116, 144),
                type_color: Rgb(149, 56, 0),
                number: Rgb(130, 80, 223),
                string: Rgb(17, 99, 41),
                operator: Rgb(87, 96, 106),
                omitted: Rgb(110, 119, 129),
                success: Rgb(26, 127, 55),
                warning: Rgb(154, 103, 0),
                error: Rgb(207, 34, 46),
            },
            ThemeId::Monochrome => Self {
                id,
                canvas_bg: Rgb(0, 0, 0),
                panel_bg: Rgb(0, 0, 0),
                header_bg: Rgb(0, 0, 0),
                active_line_bg: Rgb(0, 0, 0),
                error_bg: Rgb(0, 0, 0),
                focus_border: Rgb(255, 255, 255),
                unfocused_border: Rgb(120, 120, 120),
                text: Rgb(255, 255, 255),
                muted: Rgb(140, 140, 140),
                active_line_number: Rgb(255, 255, 255),
                control: Rgb(255, 255, 255),
                action_verb: Rgb(255, 255, 255),
                anaphor: Rgb(255, 255, 255),
                type_color: Rgb(255, 255, 255),
                number: Rgb(255, 255, 255),
                string: Rgb(255, 255, 255),
                operator: Rgb(255, 255, 255),
                omitted: Rgb(140, 140, 140),
                success: Rgb(255, 255, 255),
                warning: Rgb(255, 255, 255),
                error: Rgb(255, 255, 255),
            },
        }
    }

    /// Respects `NO_COLOR` at startup.
    #[must_use]
    pub fn initial() -> Self {
        if std::env::var_os("NO_COLOR").is_some() {
            Self::from_id(ThemeId::Monochrome)
        } else {
            Self::from_id(ThemeId::Midnight)
        }
    }

    /// Paints text with a style. Monochrome never emits RGB sequences.
    #[must_use]
    pub fn paint(&self, style: Style, text: &str) -> String {
        if text.is_empty() {
            return String::new();
        }
        let monochrome = self.id == ThemeId::Monochrome;
        let mut attributes = Vec::new();
        if !monochrome {
            if let Some(color) = style.fg {
                attributes.push(color.fg_sequence());
            }
            if let Some(color) = style.bg {
                attributes.push(color.bg_sequence());
            }
        }
        if style.bold {
            attributes.push("1".to_string());
        }
        if style.dim {
            attributes.push("2".to_string());
        }
        if style.italic {
            attributes.push("3".to_string());
        }
        if style.reverse {
            attributes.push("7".to_string());
        }
        if attributes.is_empty() {
            return text.to_string();
        }
        let mut output = String::new();
        let _ = write!(output, "\x1b[{}m{text}\x1b[0m", attributes.join(";"));
        output
    }

    /// Primary text style with panel background.
    #[must_use]
    pub fn body(&self) -> Style {
        Style::on(self.text, self.canvas_bg)
    }

    /// Panel fill style.
    #[must_use]
    pub fn panel(&self) -> Style {
        Style::on(self.text, self.panel_bg)
    }

    /// Muted label style.
    #[must_use]
    pub fn muted_style(&self) -> Style {
        Style::fg(self.muted)
    }

    /// Accent pill style.
    #[must_use]
    pub fn pill(&self) -> Style {
        Style::on(self.canvas_bg, self.focus_border).bold()
    }

    /// Success pill style.
    #[must_use]
    pub fn success_pill(&self) -> Style {
        Style::on(self.header_bg, self.success).bold()
    }

    /// Error pill style.
    #[must_use]
    pub fn error_pill(&self) -> Style {
        Style::on(self.header_bg, self.error).bold()
    }

    /// Warning pill style.
    #[must_use]
    pub fn warning_pill(&self) -> Style {
        Style::on(self.header_bg, self.warning).bold()
    }
}
