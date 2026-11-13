//! OS clipboard format preservation and privacy checks, separate from UI.
use anyhow::Result;
#[cfg(not(target_os = "android"))]
use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext, common::RustImage};
use yank_core::{Clip, ClipFormat, Preferences, Store};
#[cfg(not(target_os = "android"))]
pub struct NativeClipboard(ClipboardContext);
#[cfg(target_os = "android")]
pub struct NativeClipboard;
#[cfg(not(target_os = "android"))]
impl NativeClipboard {
    pub fn new() -> Result<Self> {
        Ok(Self(
            ClipboardContext::new().map_err(|e| anyhow::anyhow!(e.to_string()))?,
        ))
    }
    pub fn sensitive(&self) -> bool {
        let Ok(formats) = self.0.available_formats() else {
            return true;
        };
        yank_core::confidential_formats(&formats)
            || formats.iter().any(|name| {
                name == "CanIncludeInClipboardHistory"
                    && self
                        .0
                        .get_buffer(name)
                        .is_ok_and(|bytes| bytes.first() == Some(&0))
            })
    }
    pub fn capture(
        &self,
        store: &Store,
        device: &str,
        prefs: &Preferences,
    ) -> Result<Option<Clip>> {
        if self.sensitive() {
            return Ok(None);
        }
        let source = active_win_pos_rs::get_active_window()
            .ok()
            .map(|w| w.app_name)
            .unwrap_or_default();
        if !prefs.excluded_apps.trim().is_empty()
            && (source.is_empty()
                || prefs.excluded_apps.lines().any(|app| {
                    !app.trim().is_empty()
                        && source.to_lowercase().contains(&app.trim().to_lowercase())
                }))
        {
            return Ok(None);
        }
        let mut formats = Vec::new();
        let text = self.0.get_text().unwrap_or_default();
        if !prefs.exclude_pattern.is_empty()
            && regex::Regex::new(&prefs.exclude_pattern)?.is_match(&text)
        {
            return Ok(None);
        }
        if !text.is_empty() {
            formats.push(ClipFormat::text(&text));
        }
        if let Ok(html) = self.0.get_html()
            && !html.is_empty()
        {
            formats.push(ClipFormat::html(&html));
        }
        if let Ok(rtf) = self.0.get_rich_text()
            && !rtf.is_empty()
        {
            formats.push(ClipFormat::rtf(&rtf));
        }
        if let Ok(image) = self.0.get_image() {
            let (w, h) = image.get_size();
            if u64::from(w) * u64::from(h) <= 16_000_000
                && let Ok(png) = image.to_png()
                && png.get_bytes().len() < 8 * 1024 * 1024
            {
                formats.push(ClipFormat {
                    format: "PNG".into(),
                    mime: Some("image/png".into()),
                    data: png.get_bytes().to_vec(),
                });
            }
        }
        if let Ok(files) = self.0.get_files()
            && !files.is_empty()
        {
            formats.push(ClipFormat::file_list(&files));
        }
        if formats.is_empty() {
            return Ok(None);
        }
        let description = if !text.is_empty() {
            text.clone()
        } else if formats
            .iter()
            .any(|f| f.mime.as_deref() == Some("image/png"))
        {
            "图片".into()
        } else {
            "文件".into()
        };
        let mut clip = Clip::from_formats(device, &description, formats);
        clip.text = text;
        clip.source_app = source;
        if clip
            .formats
            .iter()
            .any(|f| f.mime.as_deref() == Some("image/png"))
        {
            clip.kind = "image".into();
        }
        Ok(Some(store.capture(clip, device)?))
    }
    pub fn write(&self, clip: &Clip, plain: bool) -> Result<()> {
        let mut contents = Vec::new();
        if !clip.text.is_empty() {
            contents.push(ClipboardContent::Text(clip.text.clone()));
        }
        if !plain {
            for format in &clip.formats {
                if let Some(html) = format.html_value() {
                    contents.push(ClipboardContent::Html(html.into()));
                } else if let Some(rtf) = format.rtf_value() {
                    contents.push(ClipboardContent::Rtf(rtf.into()));
                } else if let Some(files) = format.file_list_paths() {
                    contents.push(ClipboardContent::Files(files));
                } else if format.mime.as_deref() == Some("image/png")
                    && let Ok(image) = clipboard_rs::RustImageData::from_bytes(&format.data)
                {
                    contents.push(ClipboardContent::Image(image));
                }
            }
        }
        anyhow::ensure!(!contents.is_empty(), "这条记录没有可写入系统剪贴板的格式");
        self.0
            .set(contents)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(())
    }
}
#[cfg(target_os = "android")]
impl NativeClipboard {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
    pub fn sensitive(&self) -> bool {
        false
    }
    pub fn capture(&self, _: &Store, _: &str, _: &Preferences) -> Result<Option<Clip>> {
        Ok(None)
    }
    pub fn write(&self, _: &Clip, _: bool) -> Result<()> {
        anyhow::bail!("请通过界面复制文字")
    }
}
