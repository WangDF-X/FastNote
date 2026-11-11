//! 图片读取、方向校正、缩略图生成与查看解码；只在 DB 工作线程调用。

use std::fs;
use std::io::Cursor;
use std::path::Path;

use fastnote_core::{
    AttachmentError, ImageMediaType, JOURNAL_ATTACHMENT_THUMB_EDGE, JOURNAL_ATTACHMENT_VIEW_EDGE,
    MAX_JOURNAL_ATTACHMENT_BYTES, MAX_JOURNAL_ATTACHMENT_PIXELS,
};
use image::codecs::jpeg::JpegEncoder;
use image::metadata::Orientation;
use image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageFormat, ImageReader, Rgb, RgbImage,
    RgbaImage,
};

/// 读取或解码图片时的可读错误；映射为稳定的附件领域码。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImageError {
    /// 超过 10MB。
    TooLarge,
    /// 不是 PNG/JPEG。
    Unsupported,
    /// 超过 4,000 万像素。
    Pixels,
    /// 解码失败或文件不可读。
    Corrupt,
}

impl ImageError {
    /// 映射为领域错误；界面只依赖稳定错误码。
    pub(crate) fn attachment_error(self) -> AttachmentError {
        match self {
            Self::TooLarge => AttachmentError::TooLarge,
            Self::Unsupported => AttachmentError::Format,
            Self::Pixels => AttachmentError::Pixels,
            Self::Corrupt => AttachmentError::Corrupt,
        }
    }
}

/// 已校验的原图与缩略图，等待写入附件表。
#[derive(Debug)]
pub(crate) struct PreparedImage {
    /// 仅 PNG/JPEG。
    pub media_type: ImageMediaType,
    /// 方向校正后的宽。
    pub width: u32,
    /// 方向校正后的高。
    pub height: u32,
    /// 原图字节，保持与源文件一致。
    pub bytes: Vec<u8>,
    /// 最长边 480px 的 JPEG 缩略图。
    pub thumbnail: Vec<u8>,
}

/// 供查看器或缩略图条使用的 RGBA 像素。
pub(crate) struct DecodedImage {
    /// 像素宽。
    pub width: u32,
    /// 像素高。
    pub height: u32,
    /// RGBA8 行优先像素。
    pub rgba: Vec<u8>,
}

/// 读取源文件并生成缩略图；原图字节不重新编码。
pub(crate) fn prepare(path: &Path) -> Result<PreparedImage, ImageError> {
    let metadata = fs::metadata(path).map_err(|_| ImageError::Corrupt)?;
    if metadata.len() == 0 || metadata.len() > MAX_JOURNAL_ATTACHMENT_BYTES as u64 {
        return Err(ImageError::TooLarge);
    }
    let bytes = fs::read(path).map_err(|_| ImageError::Corrupt)?;
    if bytes.len() > MAX_JOURNAL_ATTACHMENT_BYTES {
        return Err(ImageError::TooLarge);
    }
    let media_type = ImageMediaType::from_magic(&bytes).ok_or(ImageError::Unsupported)?;
    let (width, height, image, orientation) = decode_dynamic(&bytes, media_type)?;
    // 缩略图先缩小再旋转，避免为 4000 万像素图片额外复制一整份。
    let mut thumbnail =
        image.thumbnail(JOURNAL_ATTACHMENT_THUMB_EDGE, JOURNAL_ATTACHMENT_THUMB_EDGE);
    thumbnail.apply_orientation(orientation);
    let thumbnail = thumbnail_jpeg(&thumbnail)?;
    let (width, height) = oriented_size(width, height, orientation);
    Ok(PreparedImage {
        media_type,
        width,
        height,
        bytes,
        thumbnail,
    })
}

/// 把原图解码到最长边不超过 `max_edge` 的 RGBA，供 Slint 显示。
pub(crate) fn decode_rgba(bytes: &[u8], max_edge: u32) -> Result<DecodedImage, ImageError> {
    let media_type = ImageMediaType::from_magic(bytes).ok_or(ImageError::Unsupported)?;
    let (_, _, mut image, orientation) = decode_dynamic(bytes, media_type)?;
    if image.width() > max_edge || image.height() > max_edge {
        image = image.thumbnail(max_edge, max_edge);
    }
    image.apply_orientation(orientation);
    let rgba = image.to_rgba8();
    Ok(DecodedImage {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

/// 缩略图解码尺寸；与入库时的生成规则保持一致。
pub(crate) fn decode_thumbnail(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    decode_rgba(bytes, JOURNAL_ATTACHMENT_THUMB_EDGE)
}

/// 查看器解码尺寸；同一时刻只保留一张大图。
pub(crate) fn decode_view(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    decode_rgba(bytes, JOURNAL_ATTACHMENT_VIEW_EDGE)
}

fn decode_dynamic(
    bytes: &[u8],
    media_type: ImageMediaType,
) -> Result<(u32, u32, DynamicImage, Orientation), ImageError> {
    let format = match media_type {
        ImageMediaType::Png => ImageFormat::Png,
        ImageMediaType::Jpeg => ImageFormat::Jpeg,
    };
    let mut decoder = ImageReader::with_format(Cursor::new(bytes), format)
        .into_decoder()
        .map_err(|_| ImageError::Corrupt)?;
    let (width, height) = decoder.dimensions();
    check_dimensions(width, height)?;
    // EXIF 方向只影响显示与缩略图，不改写原图字节。
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let image = DynamicImage::from_decoder(decoder).map_err(|_| ImageError::Corrupt)?;
    if image.width() == 0 || image.height() == 0 {
        return Err(ImageError::Corrupt);
    }
    Ok((image.width(), image.height(), image, orientation))
}

/// 方向校正后的宽高；90/270 度需要交换宽高。
fn oriented_size(width: u32, height: u32, orientation: Orientation) -> (u32, u32) {
    if matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    ) {
        (height, width)
    } else {
        (width, height)
    }
}

fn check_dimensions(width: u32, height: u32) -> Result<(), ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::Corrupt);
    }
    if u64::from(width) * u64::from(height) > MAX_JOURNAL_ATTACHMENT_PIXELS {
        return Err(ImageError::Pixels);
    }
    Ok(())
}

fn thumbnail_jpeg(image: &DynamicImage) -> Result<Vec<u8>, ImageError> {
    let thumb = image.thumbnail(JOURNAL_ATTACHMENT_THUMB_EDGE, JOURNAL_ATTACHMENT_THUMB_EDGE);
    let rgb = flatten_on_white(&thumb.to_rgba8());
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 80)
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|_| ImageError::Corrupt)?;
    Ok(out)
}

/// JPEG 不支持透明；把透明区域合成到白色，避免出现黑底。
fn flatten_on_white(image: &RgbaImage) -> RgbImage {
    let mut out = RgbImage::from_pixel(image.width(), image.height(), Rgb([255, 255, 255]));
    for (x, y, pixel) in image.enumerate_pixels() {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 {
            continue;
        }
        let blend = |channel: u8| -> u8 {
            if alpha == 255 {
                channel
            } else {
                ((u32::from(channel) * alpha + 255 * (255 - alpha)) / 255) as u8
            }
        };
        out.put_pixel(
            x,
            y,
            Rgb([blend(pixel[0]), blend(pixel[1]), blend(pixel[2])]),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;

    fn write_png(path: &Path) {
        RgbImage::from_pixel(64, 32, Rgb([10, 20, 30]))
            .save(path)
            .unwrap();
    }

    fn write_jpeg(path: &Path) {
        let image = RgbImage::from_pixel(80, 40, Rgb([200, 100, 50]));
        let mut file = std::fs::File::create(path).unwrap();
        JpegEncoder::new_with_quality(&mut file, 90)
            .encode(
                image.as_raw(),
                image.width(),
                image.height(),
                ExtendedColorType::Rgb8,
            )
            .unwrap();
    }

    #[test]
    fn test_prepare_png_and_jpeg_keeps_original_and_generates_thumbnail() {
        let directory = tempfile::tempdir().unwrap();
        for (name, writer, media_type, expected) in [
            (
                "fake.png",
                write_png as fn(&Path),
                ImageMediaType::Png,
                (64, 32),
            ),
            (
                "fake.jpg",
                write_jpeg as fn(&Path),
                ImageMediaType::Jpeg,
                (80, 40),
            ),
        ] {
            let path = directory.path().join(name);
            writer(&path);
            let original = fs::read(&path).unwrap();
            let prepared = prepare(&path).unwrap();
            assert_eq!(prepared.media_type, media_type);
            assert_eq!(prepared.bytes, original);
            assert_eq!((prepared.width, prepared.height), expected);
            assert_eq!(&prepared.thumbnail[..3], &[0xFF, 0xD8, 0xFF]);
            assert!(!prepared.thumbnail.is_empty());
        }
    }

    #[test]
    fn test_prepare_rejects_unknown_non_image_and_missing_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fake.txt");
        fs::write(&path, b"not an image").unwrap();
        assert_eq!(prepare(&path).unwrap_err(), ImageError::Unsupported);
        assert_eq!(
            prepare(&directory.path().join("missing.png")).unwrap_err(),
            ImageError::Corrupt
        );
    }

    #[test]
    fn test_dimension_guard_rejects_zero_and_oversized_pixels() {
        assert_eq!(check_dimensions(0, 10), Err(ImageError::Corrupt));
        assert_eq!(check_dimensions(10, 0), Err(ImageError::Corrupt));
        assert_eq!(check_dimensions(10_000, 10_000), Err(ImageError::Pixels));
        assert_eq!(check_dimensions(4000, 3000), Ok(()));
    }

    #[test]
    fn test_rotated_orientation_swaps_reported_dimensions() {
        assert_eq!(
            oriented_size(4000, 3000, Orientation::Rotate90),
            (3000, 4000)
        );
        assert_eq!(
            oriented_size(4000, 3000, Orientation::Rotate270FlipH),
            (3000, 4000)
        );
        assert_eq!(
            oriented_size(4000, 3000, Orientation::FlipHorizontal),
            (4000, 3000)
        );
    }

    #[test]
    fn test_thumbnail_decode_is_bounded_and_matches_rgba_length() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fake.png");
        write_png(&path);
        let prepared = prepare(&path).unwrap();
        let decoded = decode_thumbnail(&prepared.thumbnail).unwrap();
        assert!(decoded.width <= JOURNAL_ATTACHMENT_THUMB_EDGE);
        assert!(decoded.height <= JOURNAL_ATTACHMENT_THUMB_EDGE);
        assert_eq!(
            decoded.rgba.len(),
            (decoded.width * decoded.height * 4) as usize
        );
        let view = decode_view(&prepared.bytes).unwrap();
        assert_eq!((view.width, view.height), (64, 32));
    }

    #[test]
    fn test_alpha_is_flattened_on_white_for_jpeg_thumbnail() {
        let image = RgbaImage::from_pixel(2, 1, image::Rgba([0, 0, 0, 0]));
        let rgb = flatten_on_white(&image);
        assert_eq!(rgb.get_pixel(0, 0), &Rgb([255, 255, 255]));
    }
}
