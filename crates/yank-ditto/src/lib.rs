//! Independently implemented Ditto network adapter, hosted by rutis.
//! Protocol references and implementation limits are recorded in docs/ditto-protocol.md.
mod crypto;
use anyhow::{Result, bail, ensure};
use rutis::{Ctx, Plugin};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use yank_core::{Clip, ClipFormat};
const HEADER_BYTES: usize = 360;
const ENCRYPTED_HEADER: usize = 508;
const LIMIT: usize = 16 * 1024 * 1024;
/// The application owns how received clips enter local storage.
pub struct ClipSink(pub Arc<dyn Fn(Clip) + Send + Sync>);
#[derive(Clone)]
pub struct DittoPlugin {
    pub listen: SocketAddr,
    pub password: String,
}
impl Plugin for DittoPlugin {
    type Deps = Arc<ClipSink>;
    async fn apply(&self, ctx: &Ctx, sink: Self::Deps) -> rutis::Result {
        if self.password.is_empty() {
            return Err(rutis::Error::msg("Ditto password must not be empty"));
        }
        let listener = TcpListener::bind(self.listen).await?;
        let password = self.password.clone();
        let owner = ctx.clone();
        ctx.effect(move||{
            let task=tokio::spawn(async move{
                let permits=Arc::new(tokio::sync::Semaphore::new(4));let mut connections=tokio::task::JoinSet::new();
                loop {tokio::select!{
                    _=owner.cancelled()=>break,
                    value=listener.accept()=>if let Ok((socket,_))=value
                        && let Ok(permit)=permits.clone().try_acquire_owned(){let password=password.clone();let sink=sink.clone();connections.spawn(async move{let _permit=permit;let _=tokio::time::timeout(Duration::from_secs(45),receive(socket,password,sink)).await;});
                    }
                }while connections.try_join_next().is_some(){}}
                connections.abort_all();
            });async move{let _=task.await;Ok(())}
        })?;
        Ok(())
    }
}
async fn encrypted_read(socket: &mut TcpStream, count: usize, password: &str) -> Result<Vec<u8>> {
    ensure!((156..=LIMIT).contains(&count), "Ditto block exceeds limit");
    let mut data = vec![0; count];
    socket.read_exact(&mut data).await?;
    let password = password.to_owned();
    tokio::task::spawn_blocking(move || crypto::decrypt(&data, &password)).await?
}
async fn receive(mut socket: TcpStream, password: String, sink: Arc<ClipSink>) -> Result<()> {
    let mut current: Option<Clip> = None;
    let mut expected_end = false;
    let mut total = 0usize;
    loop {
        let raw = encrypted_read(&mut socket, ENCRYPTED_HEADER, &password).await?;
        ensure!(
            raw.len() == HEADER_BYTES
                && i32_at(&raw, 0)? == HEADER_BYTES as i32
                && i32_at(&raw, 8)? == 1,
            "Unsupported Ditto header"
        );
        match i32_at(&raw, 4)? {
            0 => {
                ensure!(current.is_none(), "Nested Ditto clip");
                current = Some(Clip::from_formats(
                    "ditto",
                    &c_string(&raw[48..298]),
                    Vec::new(),
                ));
                total = 0;
            }
            2 => {
                ensure!(!expected_end, "Missing data end");
                let clip = current
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("Missing clip start"))?;
                ensure!(clip.formats.len() < 32, "Too many formats");
                let size = usize::try_from(i32_at(&raw, 300)?)?;
                total = total
                    .checked_add(size)
                    .ok_or_else(|| anyhow::anyhow!("Size overflow"))?;
                ensure!(total <= LIMIT, "Clip too large");
                let data = encrypted_read(&mut socket, size, &password).await?;
                let name = c_string(&raw[48..298]);
                if name == "CF_UNICODETEXT" {
                    ensure!(data.len() % 2 == 0, "Malformed Unicode text");
                    let units: Vec<u16> = data
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|v| u16::from_le_bytes([v[0], v[1]]))
                        .take_while(|v| *v != 0)
                        .collect();
                    clip.text = String::from_utf16(&units)?;
                    clip.formats.push(ClipFormat::text(&clip.text));
                } else {
                    let mime = match name.as_str() {
                        "HTML Format" => Some("text/html"),
                        "Rich Text Format" => Some("text/rtf"),
                        "PNG" => Some("image/png"),
                        _ => None,
                    };
                    clip.formats.push(ClipFormat {
                        format: name,
                        mime: mime.map(str::to_owned),
                        data,
                    });
                }
                expected_end = true;
            }
            3 => {
                ensure!(expected_end, "Unexpected data end");
                expected_end = false;
            }
            4 => {
                ensure!(!expected_end, "Missing data end");
                let mut clip = current
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("Missing clip"))?;
                ensure!(!clip.formats.is_empty(), "Empty Ditto clip");
                if clip.description.is_empty() {
                    clip.description = clip
                        .text
                        .lines()
                        .next()
                        .unwrap_or("Ditto record")
                        .chars()
                        .take(100)
                        .collect();
                }
                (sink.0)(clip);
            }
            5 => return Ok(()),
            _ => bail!("Unsupported Ditto operation"),
        }
    }
}
/// Send a clip to a Ditto friend. Ditto is push-based; this does not request old history.
pub async fn send(address: &str, password: &str, clip: &Clip) -> Result<()> {
    ensure!(!password.is_empty(), "Ditto password must not be empty");
    let mut socket =
        tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(address)).await??;
    write_header(&mut socket, password, 0, &clip.description, -1).await?;
    for format in &clip.formats {
        let (name, data) = if let Some(text) = format.text_value() {
            (
                "CF_UNICODETEXT",
                text.encode_utf16()
                    .chain(std::iter::once(0))
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
        } else {
            (
                match format.mime.as_deref() {
                    Some("text/html") => "HTML Format",
                    Some("text/rtf") => "Rich Text Format",
                    _ => &format.format,
                },
                format.data.clone(),
            )
        };
        ensure!(
            !data.is_empty() && data.len() < LIMIT - 1024,
            "Ditto format exceeds limit"
        );
        let encrypted = crypto::encrypt(&data, password)?;
        write_header(
            &mut socket,
            password,
            2,
            name,
            i32::try_from(encrypted.len())?,
        )
        .await?;
        socket.write_all(&encrypted).await?;
        write_header(&mut socket, password, 3, "", -1).await?;
    }
    write_header(&mut socket, password, 4, "", -1).await?;
    write_header(&mut socket, password, 5, "", -1).await?;
    socket.shutdown().await?;
    Ok(())
}
async fn write_header(
    socket: &mut TcpStream,
    password: &str,
    kind: i32,
    description: &str,
    size: i32,
) -> Result<()> {
    let mut raw = vec![0u8; HEADER_BYTES];
    raw[0..4].copy_from_slice(&(HEADER_BYTES as i32).to_le_bytes());
    raw[4..8].copy_from_slice(&kind.to_le_bytes());
    raw[8..12].copy_from_slice(&1i32.to_le_bytes());
    let bytes = description.as_bytes();
    let count = bytes.len().min(249);
    raw[48..48 + count].copy_from_slice(&bytes[..count]);
    raw[300..304].copy_from_slice(&size.to_le_bytes());
    raw[304..308].copy_from_slice(&(-1i32).to_le_bytes());
    raw[340] = 1;
    socket.write_all(&crypto::encrypt(&raw, password)?).await?;
    Ok(())
}
fn i32_at(bytes: &[u8], at: usize) -> Result<i32> {
    Ok(i32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(|| anyhow::anyhow!("Truncated header"))?
            .try_into()?,
    ))
}
fn c_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len())])
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn encrypted_push_preserves_unicode_and_rich_formats() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sink = Arc::new(ClipSink(Arc::new(move |clip| {
            let _ = tx.send(clip);
        })));
        let receiver = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            receive(socket, "shared-test-password".into(), sink)
                .await
                .unwrap();
        });
        let formats = vec![
            ClipFormat::text("测试 😀"),
            ClipFormat::html("<b>测试 😀</b>"),
            ClipFormat::rtf("{\\rtf1 test}"),
        ];
        let clip = Clip::from_formats("test", "测试 😀", formats);
        send(&address.to_string(), "shared-test-password", &clip)
            .await
            .unwrap();
        let received = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received.text, "测试 😀");
        assert_eq!(received.formats.len(), 3);
        assert_eq!(received.formats[1].html_value(), Some("<b>测试 😀</b>"));
        receiver.await.unwrap();
    }
    #[tokio::test]
    async fn disposing_plugin_releases_its_listener() {
        use rutis::host::*;
        let reserved = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = reserved.local_addr().unwrap();
        drop(reserved);
        let root = Ctx::root().unwrap();
        root.provide(ClipSink(Arc::new(|_| {}))).unwrap();
        let plugin = root.plugin(DittoPlugin {
            listen: address,
            password: "test-only-password".into(),
        });
        (&plugin).await.unwrap();
        assert!(
            TcpListener::bind(address).await.is_err(),
            "active plugin owns the listener"
        );
        tokio::time::timeout(Duration::from_secs(3), plugin.dispose())
            .await
            .unwrap()
            .unwrap();
        assert!(
            TcpListener::bind(address).await.is_ok(),
            "disposal closes the listening socket"
        );
        root.shutdown().await.unwrap();
    }
}
