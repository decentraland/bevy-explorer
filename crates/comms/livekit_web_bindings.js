let currentMicTrack = null;
const activeRooms = new Set();

// Store audio elements and panner nodes for spatial audio (room-specific)
const participantAudioNodes = new Map(); // key: "roomName-participantIdentity"

// Store available track publications for subscription management
const availableTrackPublications = new Map();

export async function connect_room(url, token) {
    const room = new LivekitClient.Room({
        autoSubscribe: false,
        adaptiveStream: false,
        dynacast: false,
    });
    
    await room.connect(url, token);
    
    // Add to active rooms set
    activeRooms.add(room);
    
    // Don't automatically set up microphone - let it be controlled by the mic state
    
    return room;
}

export function set_microphone_enabled(enabled) {
    if (activeRooms.size === 0) {
        console.warn('No rooms available for microphone control');
        return;
    }
    
    if (enabled) {
        // Enable microphone
        if (!currentMicTrack) {
            LivekitClient.createLocalAudioTrack({
                echoCancellation: true,
                noiseSuppression: true,
                autoGainControl: true,
            }).then(audioTrack => {
                currentMicTrack = audioTrack;
                
                // Publish to all active rooms
                const publishPromises = Array.from(activeRooms).map(room => 
                    room.localParticipant.publishTrack(audioTrack, {
                        source: LivekitClient.Track.Source.Microphone,
                    }).catch(error => {
                        console.error(`Failed to publish to room: ${error}`);
                    })
                );
                
                return Promise.all(publishPromises);
            }).then(() => {
                console.log('Microphone enabled successfully for all rooms');
            }).catch(error => {
                console.error('Failed to enable microphone:', error);
                currentMicTrack = null;
            });
        }
    } else {
        // Disable microphone
        if (currentMicTrack) {
            // Unpublish from all active rooms
            const unpublishPromises = Array.from(activeRooms).map(room => 
                room.localParticipant.unpublishTrack(currentMicTrack).catch(error => {
                    console.error(`Failed to unpublish from room: ${error}`);
                })
            );
            
            Promise.all(unpublishPromises).then(() => {
                currentMicTrack.stop();
                currentMicTrack = null;
                console.log('Microphone disabled successfully for all rooms');
            }).catch(error => {
                console.error('Failed to disable microphone:', error);
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
    
    // Clean up participant audio nodes for this specific room only
    for (const [key, nodes] of participantAudioNodes) {
        if (key.startsWith(`${room.name}-`)) {
            // Clean up the audio nodes
            if (nodes.source) nodes.source.disconnect();
            if (nodes.pannerNode) nodes.pannerNode.disconnect();
            if (nodes.gainNode) nodes.gainNode.disconnect();
            if (nodes.audioContext) nodes.audioContext.close();
            if (nodes.track && nodes.audioElement) {
                nodes.track.detach(nodes.audioElement);
            }
            participantAudioNodes.delete(key);
        }
    }
    
    // Clean up available track publications for this room
    for (const [key, publication] of availableTrackPublications) {
        if (key.startsWith(`${room.name}-`)) {
            availableTrackPublications.delete(key);
        }
    }
    
    // If this was the last room and mic is active, clean up
    if (activeRooms.size === 0 && currentMicTrack) {
        currentMicTrack.stop();
        currentMicTrack = null;
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
        if (publication.source === LivekitClient.Track.Source.Microphone) {
            // Store the track publication for potential subscription
            const key = `${room.name}-${participant.identity}`;
            availableTrackPublications.set(key, publication);
            
            handler({
                type: 'trackPublished',
                participant: {
                    identity: participant.identity,
                    metadata: participant.metadata || ''
                }
            });
        }
    });
    
    room.on(LivekitClient.RoomEvent.TrackUnpublished, (publication, participant) => {
        if (publication.source === LivekitClient.Track.Source.Microphone) {
            // Remove the track publication
            const key = `${room.name}-${participant.identity}`;
            availableTrackPublications.delete(key);
            
            handler({
                type: 'trackUnpublished',
                participant: {
                    identity: participant.identity,
                    metadata: participant.metadata || ''
                }
            });
        }
    });
    
    room.on(LivekitClient.RoomEvent.TrackSubscribed, (track, publication, participant) => {
        // For audio tracks, set up spatial audio
        if (track.kind === 'audio') {
            const participantKey = `${room.name}-${participant.identity}`;
            console.log(`TrackSubscribed: Setting up audio for ${participantKey}`);
            
            // Check if we already have audio nodes for this participant in this room
            if (!participantAudioNodes.has(participantKey)) {
                try {
                    const audioElement = track.attach();
                    console.log(`Audio element created for ${participantKey}`, audioElement);
                    
                    // Create Web Audio API nodes for spatial audio
                    const audioContext = new (window.AudioContext || window.webkitAudioContext)();
                    
                    // Resume audio context if it's suspended (required for some browsers)
                    if (audioContext.state === 'suspended') {
                        audioContext.resume().then(() => {
                            console.log(`Audio context resumed for ${participantKey}`);
                        });
                    }
                    
                    const source = audioContext.createMediaElementSource(audioElement);
                    const pannerNode = audioContext.createStereoPanner();
                    const gainNode = audioContext.createGain();
                    
                    // Connect the audio graph: source -> panner -> gain -> destination
                    source.connect(pannerNode);
                    pannerNode.connect(gainNode);
                    gainNode.connect(audioContext.destination);
                    
                    // Store the nodes for later control with room-specific key
                    participantAudioNodes.set(participantKey, {
                        audioElement,
                        audioContext,
                        source,
                        pannerNode,
                        gainNode,
                        track,
                        roomName: room.name
                    });
                    
                    // Start playing with user interaction handling
                    audioElement.play().then(() => {
                        console.log(`Audio started playing for ${participantKey}`);
                    }).catch(e => {
                        console.warn(`Failed to play audio for ${participantKey}:`, e);
                        // Try to play again after a short delay
                        setTimeout(() => {
                            audioElement.play().catch(e2 => console.warn(`Second attempt failed for ${participantKey}:`, e2));
                        }, 1000);
                    });
                    
                } catch (error) {
                    console.error(`Failed to set up audio nodes for ${participantKey}:`, error);
                }
            } else {
                console.log(`Audio nodes already exist for participant ${participant.identity} in room ${room.name}, skipping setup`);
            }
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
        // Clean up spatial audio nodes
        if (track.kind === 'audio') {
            const participantKey = `${room.name}-${participant.identity}`;
            const nodes = participantAudioNodes.get(participantKey);
            if (nodes) {
                nodes.source.disconnect();
                nodes.pannerNode.disconnect();
                nodes.gainNode.disconnect();
                nodes.audioContext.close();
                track.detach(nodes.audioElement);
                participantAudioNodes.delete(participantKey);
            }
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
        // Clean up any audio nodes when participant disconnects from this room
        const participantKey = `${room.name}-${participant.identity}`;
        const nodes = participantAudioNodes.get(participantKey);
        if (nodes) {
            nodes.source.disconnect();
            nodes.pannerNode.disconnect();
            nodes.gainNode.disconnect();
            nodes.audioContext.close();
            participantAudioNodes.delete(participantKey);
        }
        
        handler({
            type: 'participantDisconnected',
            participant: {
                identity: participant.identity,
                metadata: participant.metadata || ''
            }
        });
    });
}

// New function to manage track subscriptions
export function set_track_subscription(room, participantIdentity, shouldSubscribe) {
    const key = `${room.name}-${participantIdentity}`;
    const publication = availableTrackPublications.get(key);
    
    if (publication) {
        publication.setSubscribed(shouldSubscribe);
        console.log(`${shouldSubscribe ? 'Subscribed to' : 'Unsubscribed from'} voice track for player ${participantIdentity} room ${room.name}`);
        return true;
    } else {
        console.warn(`No available track publication found for player ${participantIdentity}`);
        return false;
    }
}

// Spatial audio control functions with room-specific versions
export function set_participant_spatial_audio(participantIdentity, pan, volume, roomName = null) {
    let found = false;
    for (const [key, nodes] of participantAudioNodes) {
        if (key.endsWith(`-${participantIdentity}`) && (roomName === null || key.startsWith(`${roomName}-`))) {
            try {
                // Pan value should be between -1 (left) and 1 (right)
                nodes.pannerNode.pan.value = Math.max(-1, Math.min(1, pan));
                // Volume should be between 0 and 1 (or higher for boost)
                nodes.gainNode.gain.value = Math.max(0, volume);
                
                console.log(`Set spatial audio for ${participantIdentity} in room ${nodes.roomName}: pan=${pan}, volume=${volume}`);
                found = true;
                if (roomName !== null) break; // If specific room, only affect that one
            } catch (error) {
                console.error(`Failed to set spatial audio for ${participantIdentity}:`, error);
            }
        }
    }
    if (!found) {
        console.warn(`No audio nodes found for participant ${participantIdentity}${roomName ? ` in room ${roomName}` : ''}`);
    }
}

// Set pan value only (-1 to 1, where -1 is left, 0 is center, 1 is right)
export function set_participant_pan(participantIdentity, pan, roomName = null) {
    for (const [key, nodes] of participantAudioNodes) {
        if (key.endsWith(`-${participantIdentity}`) && (roomName === null || key.startsWith(`${roomName}-`))) {
            try {
                nodes.pannerNode.pan.value = Math.max(-1, Math.min(1, pan));
                if (roomName !== null) break;
            } catch (error) {
                console.error(`Failed to set pan for ${participantIdentity}:`, error);
            }
        }
    }
}

// Set volume only (0 to 1, or higher for boost)
export function set_participant_volume(participantIdentity, volume, roomName = null) {
    for (const [key, nodes] of participantAudioNodes) {
        if (key.endsWith(`-${participantIdentity}`) && (roomName === null || key.startsWith(`${roomName}-`))) {
            try {
                nodes.gainNode.gain.value = Math.max(0, volume);
                if (roomName !== null) break;
            } catch (error) {
                console.error(`Failed to set volume for ${participantIdentity}:`, error);
            }
        }
    }
}

// Get all active participant identities with audio (optionally filtered by room)
export function get_audio_participants(roomName = null) {
    const participants = [];
    for (const [key, nodes] of participantAudioNodes) {
        if (roomName === null || key.startsWith(`${roomName}-`)) {
            const participantIdentity = key.split('-').slice(1).join('-');
            participants.push({
                identity: participantIdentity,
                room: nodes.roomName,
                key: key
            });
        }
    }
    return participants;
}

// Debug function to check audio state
export function debug_audio_state() {
    console.log('Active audio participants:', Array.from(participantAudioNodes.keys()));
    for (const [key, nodes] of participantAudioNodes) {
        console.log(`${key}:`, {
            audioContext: nodes.audioContext.state,
            audioElement: {
                paused: nodes.audioElement.paused,
                muted: nodes.audioElement.muted,
                volume: nodes.audioElement.volume
            },
            gain: nodes.gainNode.gain.value,
            pan: nodes.pannerNode.pan.value
        });
    }
}

// Helper function to clean up audio resources
export function cleanup_audio_track(track) {
    if (track._audioContext) {
        track._audioContext.close();
    }
    if (track._scriptNode) {
        track._scriptNode.disconnect();
    }
    if (track._audioElement) {
        track.detach(track._audioElement);
    }
}
