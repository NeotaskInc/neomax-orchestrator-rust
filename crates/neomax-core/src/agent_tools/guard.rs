use crate::{Error, Result};

pub const DEFAULT_MAX_DEPTH: u32 = 4;
pub const MAX_ALLOWED_DEPTH: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecursionGuard {
    depth: u32,
    max_depth: u32,
}

impl RecursionGuard {
    pub fn root() -> Self {
        Self {
            depth: 0,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }

    pub fn new(depth: u32, max_depth: u32) -> Result<Self> {
        if max_depth == 0 || max_depth > MAX_ALLOWED_DEPTH {
            return Err(Error::InvalidArgument(format!(
                "tool recursion max depth must be between 1 and {MAX_ALLOWED_DEPTH}"
            )));
        }
        if depth > max_depth {
            return Err(Error::InvalidArgument(format!(
                "tool recursion depth {depth} exceeds max depth {max_depth}"
            )));
        }
        Ok(Self { depth, max_depth })
    }

    pub fn from_environment(depth: Option<&str>, max_depth: Option<&str>) -> Result<Self> {
        let depth = parse_value(depth, "NEOMAX_TOOL_DEPTH")?.unwrap_or(0);
        let max_depth =
            parse_value(max_depth, "NEOMAX_TOOL_MAX_DEPTH")?.unwrap_or(DEFAULT_MAX_DEPTH);
        Self::new(depth, max_depth)
    }

    pub fn enter(self) -> Result<Self> {
        if self.depth >= self.max_depth {
            return Err(Error::Conflict(format!(
                "tool recursion depth {} reached max depth {}",
                self.depth, self.max_depth
            )));
        }
        Ok(Self {
            depth: self.depth + 1,
            max_depth: self.max_depth,
        })
    }

    pub const fn depth(self) -> u32 {
        self.depth
    }

    pub const fn max_depth(self) -> u32 {
        self.max_depth
    }
}

fn parse_value(value: Option<&str>, name: &str) -> Result<Option<u32>> {
    value
        .map(|value| {
            value.parse::<u32>().map_err(|error| {
                Error::InvalidArgument(format!("{name} must be an unsigned integer: {error}"))
            })
        })
        .transpose()
}
