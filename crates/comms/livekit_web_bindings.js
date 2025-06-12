let currentMicTrack = null;
const activeRooms = new Set();

export async function connect_room(url, token) {
    const room = new LivekitClient.Room({
        autoSubscribe: true,
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
    
    // If this was the last room and mic is active, clean up
    if (activeRooms.size === 0 && currentMicTrack) {
        currentMicTrack.stop();
        currentMicTrack = null;
    }
    
    await room.disconnect();
}

// Remove create_audio_track and send_audio_frame functions as they're no longer needed

export function set_room_event_handler(room, handler) {
    room.on(LivekitClient.RoomEvent.DataReceived, (payload, participant) => {
        handler({
            type: 'data_received',
            payload: payload,
            participant: participant,
        });
    });
    
    room.on(LivekitClient.RoomEvent.TrackSubscribed, (track, publication, participant) => {
        // For audio tracks, automatically play them
        if (track.kind === 'audio') {
            const audioElement = track.attach();
            audioElement.play().catch(e => console.warn('Failed to play audio:', e));
            
            // Store the audio element reference on the track for cleanup
            track._audioElement = audioElement;
        }
        
        handler({
            type: 'track_subscribed',
            track: track,
            publication: publication,
            participant: participant,
        });
    });
    
    room.on(LivekitClient.RoomEvent.TrackUnsubscribed, (track, publication, participant) => {
        // Clean up audio elements
        if (track._audioElement) {
            track.detach(track._audioElement);
            track._audioElement = null;
        }
        
        handler({
            type: 'track_unsubscribed',
            track: track,
            publication: publication,
            participant: participant,
        });
    });
    
    room.on(LivekitClient.RoomEvent.ParticipantConnected, (participant) => {
        handler({
            type: 'participant_connected',
            participant: participant,
        });
    });
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
