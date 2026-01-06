function log(...args) {
    console.log("[livekit]", ...args)
}
function warn(...args) {
    console.warn("[livekit]", ...args)
}
function error(...args) {
    console.error("[livekit]", ...args)
}

let currentMicTrack = false;
const activeRooms = new Map();

// Store audio elements and panner nodes for spatial audio
const trackRigs = new Map();
const participantAudioSids = new Map();

/**
 * 
 * @returns boolean
 */
export function is_microphone_available() {
    // Check if getUserMedia is available
    const res = !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia);
    return res;
}

/**
 * 
 * @param {string} url
 * @param {string} token
 * @param {livekit.RoomOptions} room_options 
 * @param {livekit.RoomConnectOptions} room_connect_options 
 * @param {function} handler 
 * @returns livekit.Room
 */
export async function room_connect(url, token, room_options, room_connect_options, handler) {
    const room = new LivekitClient.Room(room_options);

    alt_set_room_event_handler(room, handler);

    await room.connect(url, token, room_connect_options);

    return room;
}

/**
 * 
 * @param {livekit.Room} room
 */
export async function room_close(room) {
    await room.disconnect();
}

/**
 * 
 * @param {livekit.Room} room
 * @returns string
 */
export function room_name(room) {
    return room.name
}

/**
 * 
 * @param {livekit.Room} room
 * @returns livekit.LocalParticipant
 */
export function room_local_participant(room) {
    return room.localParticipant;
}

/**
 * 
 * @param {livekit.Room} room 
 * @param {function} handler 
 */
function alt_set_room_event_handler(room, handler) {
    room.on(LivekitClient.RoomEvent.Connected, () => {
        const participants_with_tracks = Array
            .from(room.remoteParticipants.values())
            .filter(remote_participant => room.localParticipant.sid != remote_participant.sid)
            .map(remote_participant => {
                return {
                    participant: remote_participant,
                    tracks: Array.from(remote_participant.trackPublications.values())
                };
            });
        handler({
            type: 'connected',
            participants_with_tracks
        })
    });
    room.on(LivekitClient.RoomEvent.ConnectionStateChanged, (state) => {
        handler({
            type: 'connectionStateChanged',
            state: state
        })
    });
    room.on(
        LivekitClient.RoomEvent.DataReceived,
        (payload, participant, kind, topic) => {
            handler({
                type: 'dataReceived',
                payload,
                participant,
                kind,
                topic
            })
        }
    );
    room.on(
        LivekitClient.RoomEvent.ParticipantConnected,
        (remote_participant) => {
            handler({
                type: 'participantConnected',
                participant: remote_participant,
            })
        }
    );
    room.on(
        LivekitClient.RoomEvent.ParticipantDisconnected,
        (remote_participant) => {
            handler({
                type: 'participantDisconnected',
                participant: remote_participant,
            })
        }
    );
    room.on(
        LivekitClient.RoomEvent.ParticipantMetadataChanged,
        (prev_metadata, participant) => {
            handler({
                type: 'participantMetadataChanged',
                participant,
                old_metadata: prev_metadata,
                metadata: participant.metadata
            })
        }
    );
    room.on(
        LivekitClient.RoomEvent.TrackPublished,
        (remote_track_publication, remote_participant) => {
            handler({
                type: 'trackPublished',
                publication: remote_track_publication,
                participant: remote_participant
            })
        }
    );
    room.on(
        LivekitClient.RoomEvent.TrackUnpublished,
        (remote_track_publication, remote_participant) => {
            handler({
                type: 'trackUnpublished',
                publication: remote_track_publication,
                participant: remote_participant
            })
        }
    );
    room.on(
        LivekitClient.RoomEvent.TrackSubscribed,
        (remote_track, remote_track_publication, remote_participant) => {
            handler({
                type: 'trackSubscribed',
                track: remote_track,
                publication: remote_track_publication,
                participant: remote_participant
            })
        }
    );
    room.on(
        LivekitClient.RoomEvent.TrackUnsubscribed,
        // Note: The browser livekit docs say that the first parameter is a Livekit.Track,
        // not a Livekit.RemoteTrack, verify if there is ever an event with a local
        // track
        (remote_track, remote_track_publication, remote_participant) => {
            handler({
                type: 'trackUnsubscribed',
                track: remote_track,
                publication: remote_track_publication,
                participant: remote_participant
            })
        }
    );
}

/**
 * 
 * @param {livekit.LocalParticipant} local_participant
 * @param {Uint8Array} payload 
 * @param {livekit.DataPublishOptions} payload 
 * @returns string
 */
export async function local_participant_publish_data(local_participant, payload, data_publish_options) {
    local_participant.publishData(payload, data_publish_options).await;
}

/**
 * 
 * @param {livekit.LocalParticipant} local_participant 
 * @returns bool
 */
export function local_participant_is_local(local_participant) {
    return local_participant.isLocal();
}

/**
 * 
 * @param {livekit.LocalParticipant} local_participant 
 * @returns string
 */
export function local_participant_sid(local_participant) {
    return local_participant.sid;
}

/**
 * 
 * @param {livekit.LocalParticipant} local_participant 
 * @returns string
 */
export function local_participant_identity(local_participant) {
    return local_participant.identity;
}

/**
 * 
 * @param {livekit.LocalParticipant} local_participant 
 * @returns string
 */
export function local_participant_metadata(local_participant) {
    return local_participant.metadata;
}

/**
 * 
 * @param {livekit.LocalParticipant} remote_participant 
 * @returns bool
 */
export function remote_participant_is_local(remote_participant) {
    return remote_participant.isLocal();
}

/**
 * 
 * @param {livekit.RemoteParticipant} remote_participant 
 * @returns string
 */
export function remote_participant_sid(remote_participant) {
    return remote_participant.sid;
}

/**
 * 
 * @param {livekit.RemoteParticipant} remote_participant 
 * @returns string
 */
export function remote_participant_identity(remote_participant) {
    return remote_participant.identity;
}

/**
 * 
 * @param {livekit.RemoteParticipant} remote_participant 
 * @returns string
 */
export function remote_participant_metadata(remote_participant) {
    return remote_participant.metadata;
}

/**
 * 
 * @param {livekit.RemoteTrackPublication} remote_track_publication 
 * @returns string
 */
export function remote_track_publication_sid(remote_track_publication) {
    return remote_track_publication.trackSid;
}

/**
 * 
 * @param {livekit.RemoteTrackPublication} remote_track_publication 
 * @returns string
 */
export function remote_track_publication_kind(remote_track_publication) {
    return remote_track_publication.kind;
}

/**
 * 
 * @param {livekit.RemoteTrackPublication} remote_track_publication 
 * @returns string
 */
export function remote_track_publication_source(remote_track_publication) {
    return remote_track_publication.source;
}

export function set_microphone_enabled(enabled) {
    if (enabled) {
        // Enable microphone
        if (!currentMicTrack) {
            currentMicTrack = true;

            // Publish to all active rooms
            const publishPromises = activeRooms.forEach(async (room_name, room, map) => {
                log(`publish ${room.name}`);
                const audioTrack = await LivekitClient.createLocalAudioTrack({
                    echoCancellation: true,
                    noiseSuppression: true,
                    autoGainControl: true,
                });
                let pub = await room.localParticipant.publishTrack(audioTrack, {
                    source: LivekitClient.Track.Source.Microphone,
                }).catch(error_msg => {
                    error(`Failed to publish to room: ${error_msg}`);
                });

                // avoid race
                if (!currentMicTrack) {
                    await room.localParticipant.unpublishTrack(pub.track);
                }
            });

            Promise.all(publishPromises).then(() => {
                log('Microphone enabled successfully for all rooms');
            }).catch(error_msg => {
                error('Failed to enable microphone:', error_msg);
            });
        }
    } else {
        // Disable microphone
        if (currentMicTrack) {
            const allRoomUnpublishPromises = activeRooms.forEach(async (room_name, room, map) => {
                const audioPubs = Array.from(room.localParticipant.trackPublications.values())
                    .filter(pub => pub.kind === 'audio');

                const roomSpecificPromises = audioPubs.map(pub => {
                    try {
                        room.localParticipant.unpublishTrack(pub.track);
                        log(`unpublish ${room.name}`);
                    } catch (error_msg) {
                        error(`Failed to unpublish ${pub} from room ${room.name}:`, error_msg);
                    }
                });

                try {
                    await Promise.all(roomSpecificPromises);
                } catch (error_msg) {
                    error(`Failed to unpublish audio from room ${room.name}:`, error_msg);
                }
            });

            Promise.all(allRoomUnpublishPromises)
                .catch(error_msg => {
                    error('A critical error occurred during the unpublish-all process:', error_msg);
                })
                .finally(() => {
                    currentMicTrack = false;
                });
        }
    }
}

export async function publish_audio_track(room, track) {
    const publication = await room.localParticipant.publishTrack(track, {
        source: LivekitClient.Track.Source.Microphone,
    });
    return publication.trackSid;
}

export async function unpublish_track(room, sid) {
    const publication = room.localParticipant.trackPublications.get(sid);
    if (publication) {
        await room.localParticipant.unpublishTrack(publication.track);
    }
}

// Spatial audio control functions
export function set_participant_spatial_audio(participantIdentity, pan, volume) {
    const participantAudio = participantAudioSids.get(participantIdentity);
    if (!participantAudio) {
        log(`no rig for ${participantIdentity}`)
        return;
    }

    const nodes = trackRigs.get(participantAudio.audio);
    if (!nodes) {
        error(`no nodes for participant ${participantIdentity}, this should never happen`, audio);
        error("rigs:", trackRigs);
        return;
    }

    // Pan value should be between -1 (left) and 1 (right)
    nodes.pannerNode.pan.value = Math.max(-1, Math.min(1, pan));
    // Volume should be between 0 and 1 (or higher for boost)
    nodes.gainNode.gain.value = Math.max(0, volume);

    // nodes.analyser.getByteTimeDomainData(nodes.dataArray);

    // // Check if all values are '128' (which is digital silence)
    // let isSilent = true;
    // for (let i = 0; i < nodes.dataArray.length; i++) {
    //     if (nodes.dataArray[i] !== 128) {
    //         isSilent = false;
    //         break;
    //     }
    // }

    // log(`[${audioContext.state}] Set spatial audio for ${participantIdentity} : pan=${nodes.pannerNode.pan.value}, volume=${nodes.gainNode.gain.value}`);
}

// Get all active participant identities with audio
export function get_audio_participants() {
    return Array.from(participantAudioSids.keys());
}

export function subscribe_channel(roomName, participantId, subscribe) {
    const room = activeRooms.get(room.name);
    if (!room) {
        warn(`couldn't find room ${roomName} for subscription`);
        return;
    }

    const participant = room.remoteParticipants.get(participantId);
    if (!participant) {
        warn(`couldn't find participant ${participantId} in room ${roomName} for subscription`);
        return;
    }

    const audioPubs = Array.from(participant.trackPublications.values())
        .filter(pub => pub.kind === 'audio');

    log(`subscribing to ${audioPubs.length} audio tracks`);

    for (const pub of audioPubs) {
        log(`sub ${roomName}-${participantId}`);
        pub.setSubscribed(subscribe);
    }
}

export function streamer_subscribe_channel(roomName, subscribe_audio, subscribe_video) {
    const room = activeRooms.get(room.name);
    if (!room) {
        warn(`couldn't find room ${roomName} for subscription`);
        return;
    }

    const participant = room.remoteParticipants.values().find(participant => participant.identity.endsWith("-streamer"));
    if (!participant) {
        warn(`couldn't find streamer participant in room ${roomName} for subscription`);
        return;
    }

    const audioPubs = Array.from(participant.trackPublications.values())
        .filter(pub => pub.kind === 'audio');
    const videoPubs = Array.from(participant.trackPublications.values())
        .filter(pub => pub.kind === 'video');

    log(`subscribing to ${audioPubs.length} audio tracks and to ${videoPubs.length} video tracks`);

    for (const pub of audioPubs) {
        log(`sub(${subscribe_video}) ${roomName}-${participant.identity}`);
        pub.setSubscribed(subscribe_audio);
    }
    for (const pub of videoPubs) {
        log(`video sub(${subscribe_video}) ${roomName}-${participant.identity}`);
        pub.setSubscribed(subscribe_video);
    }
}
