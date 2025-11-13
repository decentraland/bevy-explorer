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
const activeRooms = new Set();

// Store audio elements and panner nodes for spatial audio
const trackRigs = new Map();
const participantAudioSids = new Map();
var audioContext = null;

export async function connect_room(url, token) {
    const room = new LivekitClient.Room({
        adaptiveStream: false,
        dynacast: false,
    });

    await room.connect(url, token, {
        autoSubscribe: false,
    });

    // Add to active rooms set
    activeRooms.add(room);

    // set up microphone
    if (currentMicTrack) {
        log(`sub ${room.name}`);
        const audioTrack = await LivekitClient.createLocalAudioTrack({
            echoCancellation: true,
            noiseSuppression: true,
            autoGainControl: true,
        });
        const pub = await room.localParticipant.publishTrack(audioTrack, {
            source: LivekitClient.Track.Source.Microphone,
        }).catch(error => {
            error(`Failed to publish to room: ${error}`);
        })

        // avoid race
        if (!currentMicTrack) {
            await room.localParticipant.unpublishTrack(pub.track);
        }
    }

    return room;
}

export function set_microphone_enabled(enabled) {
    if (enabled) {
        // Enable microphone
        if (!currentMicTrack) {
            currentMicTrack = true;

            // Publish to all active rooms
            const publishPromises = Array.from(activeRooms).map(async (room) => {
                log(`publish ${room.name}`);
                const audioTrack = await LivekitClient.createLocalAudioTrack({
                    echoCancellation: true,
                    noiseSuppression: true,
                    autoGainControl: true,
                });
                let pub = await room.localParticipant.publishTrack(audioTrack, {
                    source: LivekitClient.Track.Source.Microphone,
                }).catch(error => {
                    error(`Failed to publish to room: ${error}`);
                });

                // avoid race
                if (!currentMicTrack) {
                    await room.localParticipant.unpublishTrack(pub.track);
                }
            });

            Promise.all(publishPromises).then(() => {
                log('Microphone enabled successfully for all rooms');
            }).catch(error => {
                error('Failed to enable microphone:', error);
            });
        }
    } else {
        // Disable microphone
        if (currentMicTrack) {
            const allRoomUnpublishPromises = Array.from(activeRooms).map(async (room) => {
                const audioPubs = Array.from(room.localParticipant.trackPublications.values())
                    .filter(pub => pub.kind === 'audio');

                const roomSpecificPromises = audioPubs.map(pub => {
                    try {
                        room.localParticipant.unpublishTrack(pub.track);
                        log(`unpublish ${room.name}`);
                    } catch (error) {
                        error(`Failed to unpublish ${pub} from room ${room.name}:`, error);
                    }
                });

                try {
                    await Promise.all(roomSpecificPromises);
                } catch (error) {
                    error(`Failed to unpublish audio from room ${room.name}:`, error);
                }
            });

            Promise.all(allRoomUnpublishPromises)
                .catch(error => {
                    error('A critical error occurred during the unpublish-all process:', error);
                })
                .finally(() => {
                    currentMicTrack = false;
                });
        }
    }
}

export function is_microphone_available() {
    // Check if getUserMedia is available
    const res = !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia)
    return res;
}

export async function publish_data(room, data, reliable, destinations) {
    const options = {
        reliable: reliable,
        destination: destinations.length > 0 ? destinations : undefined,
    };

    await room.localParticipant.publishData(data, options);
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

export async function close_room(room) {
    // Remove from active rooms set
    activeRooms.delete(room);

    // If mic is active, clean up
    if (currentMicTrack) {
        const audioPubs = Array.from(room.localParticipant.trackPublications.values())
            .filter(pub => pub.kind === 'audio');

        for (const pub of audioPubs) {
            log(`stop ${room.name} on exit`);
            pub.track.stop();
        }
    }

    await room.disconnect();
}

export function set_room_event_handler(room, handler) {
    room.on(LivekitClient.RoomEvent.DataReceived, (payload, participant) => {
        handler({
            type: 'dataReceived',
            payload,
            participant: {
                identity: participant.identity,
                metadata: participant.metadata || ''
            }
        });
    });

    room.on(LivekitClient.RoomEvent.TrackPublished, (publication, participant) => {
        log(`${room.name} ${participant.identity} rec pub ${publication.kind}`);
        handler({
            type: 'trackPublished',
            kind: publication.kind,
            participant: {
                identity: participant.identity,
                metadata: participant.metadata || ''
            }
        })
    });

    room.on(LivekitClient.RoomEvent.TrackUnpublished, (publication, participant) => {
        log(`${room.name} ${participant.identity} rec unpub ${publication.kind}`);

        const key = publication.trackSid;
        const rig = trackRigs.get(key);

        if (rig) {
            log(`cleaning up audio rig for track: ${key}`);

            rig.source.disconnect();
            rig.pannerNode.disconnect();
            rig.gainNode.disconnect();

            trackRigs.delete(key);
        } else {
            log(`no cleanup for ${key}`);
        }

        handler({
            type: 'trackUnpublished',
            kind: publication.kind,
            participant: {
                identity: participant.identity,
                metadata: participant.metadata || ''
            }
        })
    });

    room.on(LivekitClient.RoomEvent.TrackSubscribed, (track, publication, participant) => {
        log(`${room.name} ${participant.identity} rec sub ${publication.kind} (track sid ${track.sid})`);
        // For audio tracks, set up spatial audio
        if (track.kind === 'audio') {
            if (!audioContext) {
                audioContext = new (window.AudioContext || window.webkitAudioContext)();
            }

            const key = track.sid;

            if (!trackRigs.has(key)) {
                log("create nodes for", key);

                // dummy audioElement
                const audioElement = track.attach();
                audioElement.volume = 0;

                // use the track internal stream in playback
                const stream = new MediaStream([track.mediaStreamTrack]);
                const source = audioContext.createMediaStreamSource(stream);
                const pannerNode = audioContext.createStereoPanner();
                const gainNode = audioContext.createGain();

                // Connect the audio graph: source -> panner -> gain -> destination
                source.connect(pannerNode);
                pannerNode.connect(gainNode);
                gainNode.connect(audioContext.destination);

                // Store the nodes for later control
                trackRigs.set(key, {
                    audioElement,
                    source,
                    pannerNode,
                    gainNode,
                    stream,
                });
            }

            const audioElement = trackRigs.get(key).audioElement;
            audioElement.play(); // we have to do this to get the stream to start pumping

            log(`set rig for ${participant.identity}`, key);
            participantAudioSids.set(participant.identity, { room: room.name, audio: key })
        }

        handler({
            type: 'trackSubscribed',
            participant: {
                identity: participant.identity,
                metadata: participant.metadata || ''
            }
        });
    });

    room.on(LivekitClient.RoomEvent.TrackUnsubscribed, (track, publication, participant) => {
        log(`${room.name} ${participant.identity} rec unsub ${publication.kind} (track sid ${track.sid})`);
        if (participantAudioSids.get(participant.identity)?.room === room.name) {
            log(`delete lookup for ${participant.identity}`);
            participantAudioSids.delete(participant.identity);
        }

        const key = track.sid;

        if (!trackRigs.has(key)) {
            log(`detach and pause audioElement for ${key}`)
            const audioElement = trackRigs(key).audioElement;
            track.detach(audioElement);
            audioElement.pause();
        }


        handler({
            type: 'trackUnsubscribed',
            participant: {
                identity: participant.identity,
                metadata: participant.metadata || ''
            }
        });
    });

    room.on(LivekitClient.RoomEvent.ParticipantConnected, (participant) => {
        handler({
            type: 'participantConnected',
            participant: {
                identity: participant.identity,
                metadata: participant.metadata || ''
            }
        });
    });

    room.on(LivekitClient.RoomEvent.ParticipantDisconnected, (participant) => {
        participantAudioSids.delete(participant.identity);
        handler({
            type: 'participantDisconnected',
            participant: {
                identity: participant.identity,
                metadata: participant.metadata || ''
            }
        });
    });
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
    const room = Array.from(activeRooms).find(room => room.name === roomName);
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

export function room_name(room) {
    return room.name
}