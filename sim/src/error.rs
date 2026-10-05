#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimError {
    pub kind: &'static str,
    pub expected: &'static str,
    pub got: &'static str,
}

impl SimError {
    pub fn new(kind: &'static str, expected: &'static str, got: &'static str) -> Self {
        Self { kind, expected, got }
    }
}

impl std::fmt::Display for SimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: expected {}, got {}", self.kind, self.expected, self.got)
    }
}

impl std::error::Error for SimError {}
