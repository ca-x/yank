//! Protobuf framing and explicit domain/wire conversions.
use crate::{
    Ack, Clip, ClipFormat, Error, MAX_WIRE_BYTES, Mutation, Page, Peer, PeerAddress, Result,
};
use prost::Message;
use yank_proto::v1 as pb;

pub trait Wire: Sized {
    type Proto: Message + Default;
    const KIND: pb::MessageKind;
    fn to_proto(&self) -> Self::Proto;
    fn from_proto(value: Self::Proto) -> Result<Self>;
}
pub fn encode<T: Wire>(value: &T) -> Result<Vec<u8>> {
    let payload = value.to_proto().encode_to_vec();
    if payload.len() > MAX_WIRE_BYTES - 64 {
        return Err(Error::Invalid("Message too large".into()));
    }
    Ok(pb::Envelope {
        protocol_version: 1,
        kind: T::KIND as i32,
        payload,
    }
    .encode_to_vec())
}
pub fn decode<T: Wire>(bytes: &[u8]) -> Result<T> {
    if bytes.len() > MAX_WIRE_BYTES {
        return Err(Error::Invalid("Message too large".into()));
    }
    let envelope = pb::Envelope::decode(bytes).map_err(|e| Error::Invalid(e.to_string()))?;
    if envelope.protocol_version != 1 || envelope.kind != T::KIND as i32 {
        return Err(Error::Invalid(
            "Unsupported protocol or message kind".into(),
        ));
    }
    T::from_proto(
        T::Proto::decode(envelope.payload.as_slice()).map_err(|e| Error::Invalid(e.to_string()))?,
    )
}
impl Wire for Clip {
    type Proto = pb::Clip;
    const KIND: pb::MessageKind = pb::MessageKind::Clip;
    fn to_proto(&self) -> pb::Clip {
        pb::Clip {
            id: self.id.clone(),
            device_id: self.device_id.clone(),
            description: self.description.clone(),
            text: self.text.clone(),
            kind: self.kind.clone(),
            collection: self.collection.clone(),
            formats: self
                .formats
                .iter()
                .map(|f| pb::ClipboardFormat {
                    format: f.format.clone(),
                    mime: f.mime.clone(),
                    data: f.data.clone(),
                })
                .collect(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted: self.deleted,
            pinned: self.pinned,
            clock: self.clock.clone(),
            source_app: self.source_app.clone(),
            version_id: self.version_id.clone(),
            write_order: self.write_order,
            write_actor: self.write_actor.clone(),
        }
    }
    fn from_proto(p: pb::Clip) -> Result<Self> {
        let clip = Self {
            id: p.id,
            device_id: p.device_id,
            description: p.description,
            text: p.text,
            kind: p.kind,
            collection: p.collection,
            formats: p
                .formats
                .into_iter()
                .map(|f| ClipFormat {
                    format: f.format,
                    mime: f.mime,
                    data: f.data,
                })
                .collect(),
            created_at: p.created_at,
            updated_at: p.updated_at,
            deleted: p.deleted,
            pinned: p.pinned,
            clock: p.clock,
            source_app: p.source_app,
            version_id: p.version_id,
            write_order: p.write_order,
            write_actor: p.write_actor,
            blob_refs: Vec::new(),
        };
        super::validate(&clip)?;
        Ok(clip)
    }
}
impl Wire for Page {
    type Proto = pb::Page;
    const KIND: pb::MessageKind = pb::MessageKind::Page;
    fn to_proto(&self) -> pb::Page {
        pb::Page {
            clips: self.clips.iter().map(Wire::to_proto).collect(),
            cursor: self.cursor,
            epoch: self.epoch.clone(),
            more: self.more,
        }
    }
    fn from_proto(p: pb::Page) -> Result<Self> {
        Ok(Self {
            clips: p
                .clips
                .into_iter()
                .map(Clip::from_proto)
                .collect::<Result<_>>()?,
            cursor: p.cursor,
            epoch: p.epoch,
            more: p.more,
        })
    }
}
impl Wire for Mutation {
    type Proto = pb::Mutation;
    const KIND: pb::MessageKind = pb::MessageKind::Mutation;
    fn to_proto(&self) -> pb::Mutation {
        pb::Mutation {
            operation: self.operation.clone(),
            clip: Some(self.clip.to_proto()),
        }
    }
    fn from_proto(p: pb::Mutation) -> Result<Self> {
        Ok(Self {
            operation: p.operation,
            clip: Clip::from_proto(
                p.clip
                    .ok_or_else(|| Error::Invalid("Missing clip".into()))?,
            )?,
        })
    }
}
impl Wire for Ack {
    type Proto = pb::Ack;
    const KIND: pb::MessageKind = pb::MessageKind::Ack;
    fn to_proto(&self) -> pb::Ack {
        pb::Ack {
            operation: self.operation.clone(),
            clip: Some(self.clip.to_proto()),
        }
    }
    fn from_proto(p: pb::Ack) -> Result<Self> {
        Ok(Self {
            operation: p.operation,
            clip: Clip::from_proto(
                p.clip
                    .ok_or_else(|| Error::Invalid("Missing clip".into()))?,
            )?,
        })
    }
}
impl Wire for Peer {
    type Proto = pb::Peer;
    const KIND: pb::MessageKind = pb::MessageKind::Peer;
    fn to_proto(&self) -> pb::Peer {
        pb::Peer {
            id: self.id.clone(),
            name: self.name.clone(),
            code: self.code.clone(),
            relay_urls: self.address.relay_urls.clone(),
            direct_addresses: self.address.direct_addresses.clone(),
        }
    }
    fn from_proto(p: pb::Peer) -> Result<Self> {
        if p.id.len() > 128
            || p.name.len() > 200
            || p.relay_urls.len() > 8
            || p.direct_addresses.len() > 32
        {
            return Err(Error::Invalid("Invalid device".into()));
        }
        Ok(Self {
            id: p.id,
            name: p.name,
            code: p.code,
            address: PeerAddress {
                relay_urls: p.relay_urls,
                direct_addresses: p.direct_addresses,
            },
        })
    }
}
impl Wire for Vec<Peer> {
    type Proto = pb::Peers;
    const KIND: pb::MessageKind = pb::MessageKind::Peers;
    fn to_proto(&self) -> pb::Peers {
        pb::Peers {
            peers: self.iter().map(Wire::to_proto).collect(),
        }
    }
    fn from_proto(p: pb::Peers) -> Result<Self> {
        p.peers.into_iter().map(Peer::from_proto).collect()
    }
}
impl Wire for () {
    type Proto = pb::Empty;
    const KIND: pb::MessageKind = pb::MessageKind::Empty;
    fn to_proto(&self) -> pb::Empty {
        pb::Empty {}
    }
    fn from_proto(_: pb::Empty) -> Result<Self> {
        Ok(())
    }
}
macro_rules! message {
    ($name:ident,$kind:ident) => {
        impl Wire for pb::$name {
            type Proto = Self;
            const KIND: pb::MessageKind = pb::MessageKind::$kind;
            fn to_proto(&self) -> Self {
                self.clone()
            }
            fn from_proto(p: Self) -> Result<Self> {
                Ok(p)
            }
        }
    };
}
message!(Credentials, Credentials);
message!(Login, Login);
message!(PeerRequest, PeerRequest);
message!(PeerResponse, PeerResponse);
message!(Announcement, Announcement);

message!(ServerInfo, ServerInfo);
