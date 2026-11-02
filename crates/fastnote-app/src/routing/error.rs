use fastnote_core::IdError;
use fastnote_ui::NavigationIssue;
use thiserror::Error;

/// 内部导航错误保留错误源，不包含用户提供的原始动作字符串。
#[derive(Debug, Error)]
pub(crate) enum RouteError {
    #[error("Unknown navigation target")]
    Target,
    #[error("Malformed navigation parameters")]
    Parameters,
    #[error("Unexpected navigation parameter")]
    UnexpectedParameter,
    #[error("Duplicate navigation parameter")]
    DuplicateParameter,
    #[error("Missing navigation parameter: {0}")]
    MissingParameter(&'static str),
    #[error("Invalid plan tab")]
    Tab,
    #[error("Invalid settings section")]
    Section,
    #[error("Invalid navigation date format")]
    DateFormat(#[source] chrono::ParseError),
    #[error("Navigation date outside supported format or range")]
    DateRange,
    #[error("Invalid navigation entity ID")]
    Id(#[source] IdError),
}

impl RouteError {
    pub(crate) fn issue(&self) -> NavigationIssue {
        match self {
            Self::Target => NavigationIssue::Target,
            Self::DateFormat(_) | Self::DateRange => NavigationIssue::Date,
            Self::Id(_) => NavigationIssue::Id,
            Self::Parameters
            | Self::UnexpectedParameter
            | Self::DuplicateParameter
            | Self::MissingParameter(_)
            | Self::Tab
            | Self::Section => NavigationIssue::Parameters,
        }
    }
}
