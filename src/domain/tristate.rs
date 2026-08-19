use std::fmt;
use std::ops::Not;

/// Kconfig tristate value: `y` / `m` / `n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tristate {
    /// `n` — disabled.
    No = 0,
    /// `m` — modular / partial.
    Module = 1,
    /// `y` — enabled.
    Yes = 2,
}

impl Tristate {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'y' | 'Y' => Some(Self::Yes),
            'm' | 'M' => Some(Self::Module),
            'n' | 'N' => Some(Self::No),
            _ => None,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "y" | "Y" | "yes" => Some(Self::Yes),
            "m" | "M" | "mod" | "module" => Some(Self::Module),
            "n" | "N" | "no" => Some(Self::No),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "y",
            Self::Module => "m",
            Self::No => "n",
        }
    }

    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::No)
    }

    /// Kconfig `&&` is the minimum of the two tristates.
    pub fn and(self, other: Self) -> Self {
        if self <= other { self } else { other }
    }

    /// Kconfig `||` is the maximum of the two tristates.
    pub fn or(self, other: Self) -> Self {
        if self >= other { self } else { other }
    }
}

impl Not for Tristate {
    type Output = Self;

    fn not(self) -> Self {
        match self {
            Self::Yes => Self::No,
            Self::Module => Self::Module,
            Self::No => Self::Yes,
        }
    }
}

impl fmt::Display for Tristate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<bool> for Tristate {
    fn from(value: bool) -> Self {
        if value { Self::Yes } else { Self::No }
    }
}

#[cfg(test)]
mod tests {
    use super::Tristate;

    #[test]
    fn and_is_minimum() {
        assert_eq!(Tristate::Yes.and(Tristate::Module), Tristate::Module);
        assert_eq!(Tristate::Module.and(Tristate::No), Tristate::No);
        assert_eq!(Tristate::Yes.and(Tristate::Yes), Tristate::Yes);
    }

    #[test]
    fn or_is_maximum() {
        assert_eq!(Tristate::No.or(Tristate::Module), Tristate::Module);
        assert_eq!(Tristate::Module.or(Tristate::Yes), Tristate::Yes);
    }

    #[test]
    fn not_inverts_yes_and_no() {
        assert_eq!(!Tristate::Yes, Tristate::No);
        assert_eq!(!Tristate::No, Tristate::Yes);
        assert_eq!(!Tristate::Module, Tristate::Module);
    }
}
