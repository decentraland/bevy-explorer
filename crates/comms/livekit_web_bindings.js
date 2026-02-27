function log(...args) {
    console.log("[livekit]", ...args)
}
function warn(...args) {
    console.warn("[livekit]", ...args)
}
function error(...args) {
    console.error("[livekit]", ...args)
}

var audioContext = null;
var microphonePermission = "denied";

export function setupMicrophonePermission() {
    navigator.permissions.query({ name: "microphone" }).then((permissionState) => {
        microphonePermission = permissionState.state;

        permissionState.onchange = () => {
            microphonePermission = permissionState.state;
        };
    });
}

/**
 * Tests if the browser can accept requests for microphone streams
 * @returns boolean
 */
export function is_microphone_available() {
    return !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia);
}

/**
 * Requests current microphone permission state
 * @returns "granted" | "prompt" | "denied"
 */
export function microphonePermissionState() {
    return microphonePermission;
}

/**
 * Prompts for microphone permission
 */
export function promptMicrophonePermission() {
    if (navigator.mediaDevices && navigator.mediaDevices.getUserMedia) {
        navigator.mediaDevices.getUserMedia({ audio: true });
    }
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

    set_room_event_handler(room, handler);

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
function set_room_event_handler(room, handler) {
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
    room.on(LivekitClient.RoomEvent.Disconnected, (disconnectReason) => {
        handler({
            type: 'disconnected',
            disconnectReason
        })
    });
    room.on(LivekitClient.RoomEvent.ConnectionStateChanged, (state) => {
        handler({
            type: 'connectionStateChanged',
            state: state
        })
    });
    room.on(LivekitClient.RoomEvent.ConnectionQualityChanged, (connection_quality, participant) => {
        handler({
            type: 'connectionQualityChanged',
            connection_quality,
            participant
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
            log(`Subscribed to track ${remote_track.sid} of ${remote_participant.sid} (${remote_participant.identity}).`);

            if (remote_track.kind === "audio") {
                if (remote_track.trackRig) {
                    error(`Rebuilding track rig of ${remote_track.sid} for ${remote_participant.sid} (${remote_participant.identity}).`);
                    track_rig_drop(remote_track);
                }
                if (!audioContext) {
                    audioContext = new (window.AudioContext || window.webkitAudioContext)();
                }

                if (remote_participant.identity.endsWith("-streamer")) {
                    if (remote_track.audioElement) {
                        error(`Rebuilding audio element of ${remote_track.sid} for ${remote_participant.sid} (${remote_participant.identity}).`);
                        const audioElement = remote_track.audioElement;
                        delete remote_track.audioElement;
                        remote_track.detach(audioElement);
                    }
                    const streamPlayerContainer = window.document.querySelector("#stream-player-container");
                    if (streamPlayerContainer) {
                        const audioElement = remote_track.attach();
                        streamPlayerContainer.append(audioElement);
                        remote_track.audioElement = audioElement;
                    }
                } else {
                    track_rig_new(remote_track);
                }
            } else if (remote_track.kind == "video") {
                if (remote_track.videoElement) {
                    error(`Rebuilding video element of ${remote_track.sid} for ${remote_participant.sid} (${remote_participant.identity}).`);
                    const videoElement = remote_track.videoElement;
                    delete remote_track.videoElement;
                    remote_track.detach(videoElement);
                }
                const streamPlayerContainer = window.document.querySelector("#stream-player-container");
                if (streamPlayerContainer) {
                    const videoElement = remote_track.attach();
                    streamPlayerContainer.append(videoElement);
                    remote_track.videoElement = videoElement;
                }
            }

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
            log(`Unsubscribed to track ${remote_track.sid} of ${remote_participant.sid} (${remote_participant.identity}).`);
            if (remote_track.kind === "audio") {
                track_rig_drop(remote_track);
            }
            if (remote_track.audioElement) {
                const audioElement = remote_track.audioElement;
                delete remote_track.audioElement;
                remote_track.detach(audioElement);
                audioElement.remove();
            }

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
 * @param {livekit.Participant} participant
 * @returns bool
 */
export async function particinpant_is_local(participant) {
    return particinpant.isLocal;
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
 * @param {livekit.LocalTrack} local_track
 * @param {livekit.TrackPublishingOptions} track_publishing_option
 * @returns livekit.LocalTrackPublication
 */
export async function local_participant_publish_track(local_participant, local_track, track_publishing_option) {
    return await local_participant.publishTrack(local_track, track_publishing_option);
}

/**
 * 
 * @param {livekit.LocalParticipant} local_participant
 * @param {livekit.LocalTrack} local_track
 * @returns livekit.LocalTrackPublication
 */
export async function local_participant_unpublish_track(local_participant, local_track) {
    return await local_participant.unpublishTrack(local_track, true);
}

/**
 * 
 * @param {livekit.LocalParticipant} local_participant 
 * @returns bool
 */
export function local_participant_is_local(local_participant) {
    return local_participant.isLocal;
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
    return remote_participant.isLocal;
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

/**
 * 
 * @param {livekit.RemoteTrackPublication} remote_track_publication 
 * @param {boolean} subscribed 
 * @returns string
 */
export function remote_track_publication_set_subscribed(remote_track_publication, subscribed) {
    remote_track_publication.setSubscribed(subscribed);
}


/**
 * 
 * @param {livekit.RemoteTrackPublication} remote_track_publication 
 * @returns livekit.RemoteTrack | null
 */
export function remote_track_publication_track(remote_track_publication) {
    log(remote_track_publication);
    return remote_track_publication.track;
}

/**
 * 
 * @param {livekit.AudioCaptureOptions} options 
 * @returns livekit.LocalAudioTrack
 */
export async function local_audio_track_new(options) {
    try {
        return await LivekitClient.createLocalAudioTrack(options);
    } catch (err) {
        error(err);
    }
}

/**
 * 
 * @param {livekit.LocalAudioTrack} local_audio_track 
 * @returns livekit.TrackSid
 */
export function local_audio_track_sid(local_audio_track) {
    return local_audio_track.sid;
}

/**
 * 
 * @param {livekit.RemoteTrack} remote_track 
 */
function track_rig_new(remote_track) {
    log(`Creating new track rig for ${remote_track.sid}.`);

    // dummy audioElement
    const audioElement = remote_track.attach();
    audioElement.volume = 0;

    // use the track internal stream in playback
    const stream = new MediaStream([remote_track.mediaStreamTrack]);
    const source = audioContext.createMediaStreamSource(stream);
    const pannerNode = audioContext.createStereoPanner();
    const gainNode = audioContext.createGain();

    // Connect the audio graph: source -> panner -> gain -> destination
    source.connect(pannerNode);
    pannerNode.connect(gainNode);
    gainNode.connect(audioContext.destination);

    // Store the nodes for later control
    remote_track.trackRig = {
        audioElement,
        source,
        pannerNode,
        gainNode,
        stream,
    };

    audioElement.play();
}

/**
 * 
 * @param {livekit.RemoteTrack} remote_track 
 */
function track_rig_drop(remote_track) {
    log(`Dropping track rig of ${remote_track.sid}.`);
    const track_rig = remote_track.trackRig;
    if (track_rig) {
        delete remote_track.trackRig;

        remote_track.detach(track_rig.audioElement);
        track_rig.source.disconnect();
        track_rig.pannerNode.disconnect();
        track_rig.gainNode.disconnect();
        track_rig.audioElement.pause();
    }
}

/**
 * 
 * @param {livekit.RemoteTrack} remote_track 
 * @param {float} pan 
 * @param {float} volume 
 */
export function remote_track_pan_and_volume(remote_track, pan, volume) {
    log(`Setting pan and volume for track ${remote_track.sid}.`);
    const track_rig = remote_track.trackRig;
    // Pan value should be between -1 (left) and 1 (right)
    track_rig.pannerNode.pan.value = Math.max(-1, Math.min(1, pan));
    // Volume should be between 0 and 1 (or higher for boost)
    track_rig.gainNode.gain.value = Math.max(0, volume);

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
