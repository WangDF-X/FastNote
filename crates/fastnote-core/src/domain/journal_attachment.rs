//! 日记图片附件：格式、大小与像素上限由领域层校验（DIARY-01 §5.1）。

use chrono::{DateTime, NaiveDate, Utc};
use thiserror::Error;

/// 单张图片的原始字节上限（10MB）。
pub const MAX_JOURNAL_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;

/// 单日最多附件数量。
pub const MAX_JOURNAL_ATTACHMENTS: u32 = 20;

/// 单张图片的最大像素数（4,000 万），用于阻止解压炸弹。
pub const MAX_JOURNAL_ATTACHMENT_PIXELS: u64 = 40_000_000;

/// 缩略图最长边（像素）。
pub const JOURNAL_ATTACHMENT_THUMB_EDGE: u32 = 480;

/// 查看器解码的最长边（像素）。
pub const JOURNAL_ATTACHMENT_VIEW_EDGE: u32 = 1600;

/// 首版支持的图片容器。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageMediaType {
    /// PNG。
    Png,
    /// JPEG。
    Jpeg,
}

impl ImageMediaType {
    /// 持久化到 `journal_attachments.media_type` 的稳定值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }

    /// 数据库中的 Media Type 转枚举；未知值视为损坏数据。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "image/png" => Some(Self::Png),
            "image/jpeg" => Some(Self::Jpeg),
            _ => None,
        }
    }

    /// 按文件头识别 PNG/JPEG，不信任扩展名。
    pub fn from_magic(bytes: &[u8]) -> Option<Self> {
        const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        if bytes.starts_with(PNG) {
            Some(Self::Png)
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else {
            None
        }
    }

    /// 导出 zip 中使用的扩展名。
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

/// 一条日记图片附件；原图与缩略图字节不放在元数据中。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalAttachment {
    /// 应用层生成的 UUID v7。
    pub id: String,
    /// 所属本地日期。
    pub entry_date: NaiveDate,
    /// 日期内的显示顺序，由仓储在插入时分配。
    pub position: u32,
    /// 仅 PNG/JPEG。
    pub media_type: ImageMediaType,
    /// 原图像素宽。
    pub width: u32,
    /// 原图像素高。
    pub height: u32,
    /// 原图字节数。
    pub byte_size: u32,
    /// 缩略图字节数。
    pub thumbnail_size: u32,
    /// 添加时间（UTC）。
    pub created_at: DateTime<Utc>,
}

impl JournalAttachment {
    /// 建立元数据；位置由仓储插入时分配，字节数由 [`Self::with_data`] 记录。
    pub fn new(
        id: String,
        entry_date: NaiveDate,
        media_type: ImageMediaType,
        width: u32,
        height: u32,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            entry_date,
            position: 0,
            media_type,
            width,
            height,
            byte_size: 0,
            thumbnail_size: 0,
            created_at: now,
        }
    }

    /// 附上原图与缩略图并校验全部上限。
    pub fn with_data(mut self, bytes: &[u8], thumbnail: &[u8]) -> Result<Self, AttachmentError> {
        self.byte_size = u32::try_from(bytes.len()).map_err(|_| AttachmentError::TooLarge)?;
        self.thumbnail_size =
            u32::try_from(thumbnail.len()).map_err(|_| AttachmentError::TooLarge)?;
        validate_attachment(&self, bytes, thumbnail)?;
        Ok(self)
    }
}

/// 图片校验的稳定错误码；与 [`crate::JournalError`] 共享界面映射。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AttachmentError {
    /// 超过单张 10MB 或无法用 u32 表示。
    #[error("E_JOURNAL_IMAGE_TOO_LARGE")]
    TooLarge,
    /// 超过单日 20 张。
    #[error("E_JOURNAL_IMAGE_LIMIT")]
    Limit,
    /// 宽高为 0 或超过 4,000 万像素。
    #[error("E_JOURNAL_IMAGE_PIXELS")]
    Pixels,
    /// 不是 PNG/JPEG，或声明的类型与魔数不一致。
    #[error("E_JOURNAL_IMAGE_FORMAT")]
    Format,
    /// 元数据与字节数不一致或缩略图为空。
    #[error("E_JOURNAL_IMAGE_CORRUPT")]
    Corrupt,
}

/// 校验附件声明的类型、尺寸、字节数与上限。
pub fn validate_attachment(
    attachment: &JournalAttachment,
    bytes: &[u8],
    thumbnail: &[u8],
) -> Result<(), AttachmentError> {
    if bytes.is_empty() || bytes.len() > MAX_JOURNAL_ATTACHMENT_BYTES {
        return Err(AttachmentError::TooLarge);
    }
    if thumbnail.is_empty() || thumbnail.len() > MAX_JOURNAL_ATTACHMENT_BYTES {
        return Err(AttachmentError::Corrupt);
    }
    if ImageMediaType::from_magic(bytes) != Some(attachment.media_type) {
        return Err(AttachmentError::Format);
    }
    if attachment.width == 0
        || attachment.height == 0
        || u64::from(attachment.width) * u64::from(attachment.height)
            > MAX_JOURNAL_ATTACHMENT_PIXELS
    {
        return Err(AttachmentError::Pixels);
    }
    if attachment.byte_size as usize != bytes.len()
        || attachment.thumbnail_size as usize != thumbnail.len()
    {
        return Err(AttachmentError::Corrupt);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()
    }

    fn now() -> DateTime<Utc> {
        "2026-11-10T06:32:00Z".parse().unwrap()
    }

    fn png_bytes() -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&[0; 16]);
        bytes
    }

    fn jpeg_bytes() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0]
    }

    fn thumbnail() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0, 0]
    }

    #[test]
    fn test_magic_detection_does_not_trust_extension() {
        assert_eq!(
            ImageMediaType::from_magic(&png_bytes()),
            Some(ImageMediaType::Png)
        );
        assert_eq!(
            ImageMediaType::from_magic(&jpeg_bytes()),
            Some(ImageMediaType::Jpeg)
        );
        assert_eq!(ImageMediaType::from_magic(b"GIF89a"), None);
        assert_eq!(ImageMediaType::from_magic(b"<svg/>"), None);
    }

    #[test]
    fn test_valid_attachment_records_byte_sizes() {
        let bytes = png_bytes();
        let attachment = JournalAttachment::new(
            "019a0fff-aaaa-7000-8000-000000000001".into(),
            date(),
            ImageMediaType::Png,
            1280,
            720,
            now(),
        )
        .with_data(&bytes, &thumbnail())
        .unwrap();
        assert_eq!(attachment.byte_size as usize, bytes.len());
        assert_eq!(attachment.thumbnail_size as usize, thumbnail().len());
        assert_eq!(attachment.position, 0);
    }

    #[test]
    fn test_large_oversized_corrupt_and_mismatched_images_are_rejected() {
        let base =
            |media_type| JournalAttachment::new("id".into(), date(), media_type, 10, 10, now());
        let too_large = vec![0; MAX_JOURNAL_ATTACHMENT_BYTES + 1];
        assert_eq!(
            base(ImageMediaType::Png).with_data(&too_large, &thumbnail()),
            Err(AttachmentError::TooLarge)
        );
        assert_eq!(
            base(ImageMediaType::Png).with_data(&png_bytes(), &[]),
            Err(AttachmentError::Corrupt)
        );
        assert_eq!(
            base(ImageMediaType::Jpeg).with_data(&png_bytes(), &thumbnail()),
            Err(AttachmentError::Format)
        );
        assert_eq!(
            JournalAttachment::new(
                "id".into(),
                date(),
                ImageMediaType::Png,
                10_000,
                10_000,
                now()
            )
            .with_data(&png_bytes(), &thumbnail()),
            Err(AttachmentError::Pixels)
        );
        assert_eq!(ImageMediaType::parse("image/webp"), None);
        assert_eq!(ImageMediaType::Png.extension(), "png");
        assert_eq!(ImageMediaType::Jpeg.as_str(), "image/jpeg");
    }
}
