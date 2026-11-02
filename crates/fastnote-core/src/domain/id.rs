//! 强类型 UUIDv7 标识（ARCH-03 §3 / AGENTS.md §3.12）。
//!
//! UUID（Universally Unique Identifier，通用唯一标识符）在文本边界统一输出为小写。
//! 本模块只校验与转换，不读取系统时钟、不生成随机数。

use std::fmt;
use std::str::FromStr;

use thiserror::Error;
use uuid::{Uuid, Variant};

/// 实体 ID 的格式或 UUIDv7 校验失败。
#[derive(Debug, Error)]
pub enum IdError {
    /// 输入不是合法 UUID；原始错误只供内部定位，不直接展示给用户。
    #[error("Invalid UUID: {0}")]
    InvalidUuid(#[from] uuid::Error),
    /// ID 不是标准变体的 UUIDv7，包括 nil、v4 和其它变体。
    #[error("Entity IDs must be RFC UUIDv7")]
    NotV7,
}

macro_rules! entity_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Uuid);

        impl $name {
            /// 借用底层 UUID，供存储边界或应用层使用。
            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = IdError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                if value.get_version_num() != 7 || value.get_variant() != Variant::RFC4122 {
                    return Err(IdError::NotV7);
                }
                Ok(Self(value))
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::try_from(Uuid::parse_str(value)?)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.hyphenated().fmt(formatter)
            }
        }
    };
}

entity_id!(PlanId, "计划的 UUIDv7 标识，不能与任务或小记 ID 混用。");
entity_id!(
    TaskId,
    "任务/阶段的 UUIDv7 标识，不能与计划或小记 ID 混用。"
);
entity_id!(NoteId, "小记的 UUIDv7 标识，不能与计划或任务 ID 混用。");

#[cfg(test)]
mod tests {
    use super::*;

    const V7: &str = "019a0fff-aaaa-7000-8000-000000000001";

    #[test]
    fn test_all_entity_ids_accept_v7() {
        let plan: PlanId = V7.parse().unwrap();
        let task: TaskId = V7.parse().unwrap();
        let note: NoteId = V7.parse().unwrap();
        assert_eq!(plan.as_uuid().get_version_num(), 7);
        assert_eq!(task.to_string(), V7);
        assert_eq!(note.to_string(), V7);
    }

    #[test]
    fn test_display_normalizes_uppercase_to_lowercase() {
        let plan: PlanId = V7.to_uppercase().parse().unwrap();
        assert_eq!(plan.to_string(), V7);
    }

    #[test]
    fn test_invalid_uuid_returns_original_error_source() {
        use std::error::Error;

        let error = "not-an-id".parse::<NoteId>().unwrap_err();
        assert!(matches!(error, IdError::InvalidUuid(_)));
        assert!(error.source().is_some());
    }

    #[test]
    fn test_v4_and_nil_are_rejected() {
        for value in [
            "550e8400-e29b-41d4-a716-446655440000",
            "00000000-0000-0000-0000-000000000000",
        ] {
            assert!(matches!(value.parse::<TaskId>(), Err(IdError::NotV7)));
        }
    }

    #[test]
    fn test_non_rfc_variant_is_rejected_even_with_v7_bits() {
        let value = "019a0fff-aaaa-7000-0000-000000000001";
        assert!(matches!(value.parse::<PlanId>(), Err(IdError::NotV7)));
    }

    #[test]
    fn test_id_round_trip_preserves_value() {
        let first: TaskId = V7.parse().unwrap();
        let second: TaskId = first.to_string().parse().unwrap();
        assert_eq!(first, second);
    }
}
