use anyhow::Context;
use std::io::Cursor;
use std::path::Path;

fn render_cover_variants(raw_cover: &[u8]) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let image = image::load_from_memory(raw_cover).context("load cover image")?;
    if image.width() == 0 || image.height() == 0 {
        anyhow::bail!("invalid cover image dimensions");
    }

    let cover = image.thumbnail(400, 600);
    let thumb = image.thumbnail(100, 150);

    let mut cover_writer = Cursor::new(Vec::new());
    cover
        .write_to(&mut cover_writer, image::ImageFormat::Jpeg)
        .context("encode cover jpeg")?;

    let mut thumb_writer = Cursor::new(Vec::new());
    thumb
        .write_to(&mut thumb_writer, image::ImageFormat::Jpeg)
        .context("encode thumb jpeg")?;

    Ok((cover_writer.into_inner(), thumb_writer.into_inner()))
}

pub fn copy_cover(source_cover_path: &Path, target_cover_path: &Path, target_thumb_path: &Path) -> anyhow::Result<()> {
    let source_bytes = std::fs::read(source_cover_path)
        .with_context(|| format!("read source cover {}", source_cover_path.display()))?;
    let (cover_bytes, thumb_bytes) = render_cover_variants(&source_bytes)?;

    if let Some(parent) = target_cover_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create cover directory {}", parent.display()))?;
    }
    if let Some(parent) = target_thumb_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create thumb directory {}", parent.display()))?;
    }

    std::fs::write(target_cover_path, cover_bytes)
        .with_context(|| format!("write cover {}", target_cover_path.display()))?;
    std::fs::write(target_thumb_path, thumb_bytes)
        .with_context(|| format!("write thumb {}", target_thumb_path.display()))?;

    Ok(())
}
