//! VoIP signaling (ADR 0003, stage 1): 1:1 call setup over Matrix
//! `m.call.*` timeline events — the spec-standard legacy flavor that
//! Element supports for 1:1 calls. Typed event contents come from
//! ruma's `events::call` module (m.call.invite/answer/candidates/
//! hangup).
//!
//! Deliberately media-free: the WebRTC engine (webrtc-rs + cpal + Opus)
//! is a separate increment gated on this layer being verified
//! end-to-end (invite -> answer -> hangup between two sessions).
//!
//! Frontends subscribe to [`CallSignal`]s and respond through the
//! send_* methods. All events are room-timeline message-like events,
//! so both peregrine sessions and Element see them.

use std::sync::Arc;

use matrix_sdk::ruma::events::call::candidates::Candidate;
use matrix_sdk::ruma::events::call::{
    answer::CallAnswerEventContent, candidates::CallCandidatesEventContent,
    hangup::CallHangupEventContent, invite::CallInviteEventContent,
};
use matrix_sdk::ruma::events::call::SessionDescription;
use matrix_sdk::ruma::{OwnedEventId, OwnedUserId, VoipVersionId};
use matrix_sdk::Client;
use matrix_sdk::ruma::OwnedRoomId;
use tokio::sync::mpsc;

use crate::protocol::RoomId;

/// One signaling event, surfaced to the frontend subscriber.
#[derive(Debug, Clone)]
pub enum CallSignal {
    /// Incoming `m.call.invite`.
    Invite {
        /// Room the call lives in.
        room: OwnedRoomId,
        /// Caller-generated call identifier.
        call_id: String,
        /// Inviting user (mxid).
        from: OwnedUserId,
        /// SDP offer.
        sdp: String,
        /// Ring timeout advertised by the caller (milliseconds).
        lifetime: u64,
    },
    /// Incoming `m.call.answer` (our invite was accepted).
    Answer {
        /// Room the call lives in.
        room: OwnedRoomId,
        /// Caller-generated call identifier.
        call_id: String,
        /// SDP answer.
        sdp: String,
    },
    /// Incoming ICE candidates for a call we're party to.
    Candidates {
        /// Room the call lives in.
        room: OwnedRoomId,
        /// Caller-generated call identifier.
        call_id: String,
        /// Raw candidate SDP "a" lines.
        candidates: Vec<String>,
    },
    /// Incoming `m.call.hangup`.
    Hangup {
        /// Room the call lives in.
        room: OwnedRoomId,
        /// Caller-generated call identifier.
        call_id: String,
        /// Hangup reason (spec enum as string) if given.
        reason: Option<String>,
    },
}

/// Signaling service bound to a logged-in client. Registers voip
/// event handlers on the client and sends m.call.* events through
/// the target room.
#[derive(Clone)]
pub struct SignalingService {
    client: Client,
}

impl SignalingService {
    /// Bind to a client: registers handlers that forward voip events
    /// into `signals`.
    pub fn new(client: Client, signals: mpsc::Sender<CallSignal>) -> Arc<Self> {
        let tx = Arc::new(signals);
        client.add_event_handler({
            let tx = tx.clone();
            move |event: matrix_sdk::ruma::events::call::invite::SyncCallInviteEvent, room: matrix_sdk::Room| {
                let tx = tx.clone();
                async move {
                    let Some(ev) = event.as_original() else { return };
                    let c = &ev.content;
                    let _ = tx
                        .send(CallSignal::Invite {
                            room: room.room_id().to_owned().into(),
                            call_id: c.call_id.to_string(),
                            from: ev.sender.clone(),
                            sdp: c.offer.sdp.clone(),
                            lifetime: u64::from(c.lifetime),
                        })
                        .await;
                }
            }
        });
        client.add_event_handler({
            let tx = tx.clone();
            move |event: matrix_sdk::ruma::events::call::answer::SyncCallAnswerEvent, room: matrix_sdk::Room| {
                let tx = tx.clone();
                async move {
                    let Some(ev) = event.as_original() else { return };
                    let c = &ev.content;
                    let _ = tx
                        .send(CallSignal::Answer {
                            room: room.room_id().to_owned().into(),
                            call_id: c.call_id.to_string(),
                            sdp: c.answer.sdp.clone(),
                        })
                        .await;
                }
            }
        });
        client.add_event_handler({
            let tx = tx.clone();
            move |event: matrix_sdk::ruma::events::call::candidates::SyncCallCandidatesEvent, room: matrix_sdk::Room| {
                let tx = tx.clone();
                async move {
                    let Some(ev) = event.as_original() else { return };
                    let c = &ev.content;
                    let _ = tx
                        .send(CallSignal::Candidates {
                            room: room.room_id().to_owned().into(),
                            call_id: c.call_id.to_string(),
                            candidates: c
                                .candidates
                                .iter()
                                .map(|candidate| candidate.candidate.clone())
                                .collect(),
                        })
                        .await;
                }
            }
        });
        client.add_event_handler({
            let tx = tx.clone();
            move |event: matrix_sdk::ruma::events::call::hangup::SyncCallHangupEvent, room: matrix_sdk::Room| {
                let tx = tx.clone();
                async move {
                    let Some(ev) = event.as_original() else { return };
                    let c = &ev.content;
                    let _ = tx
                        .send(CallSignal::Hangup {
                            room: room.room_id().to_owned().into(),
                            call_id: c.call_id.to_string(),
                            reason: Some(c.reason.as_str().to_owned()),
                        })
                        .await;
                }
            }
        });

        Arc::new(Self { client })
    }

    fn room(&self, room: &RoomId) -> Result<matrix_sdk::Room, String> {
        let rid = matrix_sdk::ruma::RoomId::parse(room.as_str())
            .map_err(|e| format!("room id parse: {e}"))?;
        self.client
            .get_room(&rid)
            .ok_or_else(|| format!("room not in state: {room}"))
    }

    /// Send an `m.call.invite` (SDP offer) into the room. Returns the
    /// generated event id.
    pub async fn send_invite(
        &self,
        room: &RoomId,
        call_id: String,
        sdp: String,
        lifetime_ms: u32,
    ) -> Result<OwnedEventId, String> {
        let room = self.room(room)?;
        let content = CallInviteEventContent::new(
            call_id.into(),
            lifetime_ms.into(),
            SessionDescription::new("offer".to_owned(), sdp),
            VoipVersionId::V1,
        );
        room.send(content)
            .await
            .map_err(|e| format!("send invite: {e}"))
            .map(|r| OwnedEventId::from(r.response.event_id))
    }

    /// Send an `m.call.answer` (SDP answer) for a call we were invited to.
    pub async fn send_answer(
        &self,
        room: &RoomId,
        call_id: String,
        sdp: String,
    ) -> Result<OwnedEventId, String> {
        let room = self.room(room)?;
        let content = CallAnswerEventContent::new(
            SessionDescription::new("answer".to_owned(), sdp),
            call_id.into(),
            VoipVersionId::V1,
        );
        room.send(content)
            .await
            .map_err(|e| format!("send answer: {e}"))
            .map(|r| OwnedEventId::from(r.response.event_id))
    }

    /// Send ICE candidates for a call (raw SDP "a" lines, index-order).
    pub async fn send_candidates(
        &self,
        room: &RoomId,
        call_id: String,
        candidates: Vec<Candidate>,
    ) -> Result<OwnedEventId, String> {
        let room = self.room(room)?;
        let content = CallCandidatesEventContent::new(
            call_id.into(),
            candidates,
            VoipVersionId::V1,
        );
        room.send(content)
            .await
            .map_err(|e| format!("send candidates: {e}"))
            .map(|r| OwnedEventId::from(r.response.event_id))
    }

    /// Send an `m.call.hangup` (version 1; user-initiated).
    pub async fn send_hangup(
        &self,
        room: &RoomId,
        call_id: String,
    ) -> Result<OwnedEventId, String> {
        let room = self.room(room)?;
        let content = CallHangupEventContent::new(call_id.into(), VoipVersionId::V1);
        room.send(content)
            .await
            .map_err(|e| format!("send hangup: {e}"))
            .map(|r| OwnedEventId::from(r.response.event_id))
    }
}
