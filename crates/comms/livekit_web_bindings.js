export async function connect_room(url, token) {
    const room = new LivekitClient.Room({
        autoSubscribe: true,
        adaptiveStream: false,
        dynacast: false,
    });
    
    await room.connect(url, token);
    return room;
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
    await room.disconnect();
}

export function create_audio_track(sampleRate, numChannels) {
    // Create a Web Audio context and source
    const audioContext = new AudioContext({ sampleRate });
    const source = audioContext.createScriptProcessor(4096, numChannels, numChannels);
    
    // Create MediaStream from the source
    const destination = audioContext.createMediaStreamDestination();
    source.connect(destination);
    
    // Create LiveKit track from the stream
    const track = LivekitClient.createLocalAudioTrack({
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
    });
    
    // Store the processor for sending audio data
    track._processor = source;
    track._context = audioContext;
    
    return track;
}

export function send_audio_frame(track, samples, sampleRate, numChannels) {
    if (track._processor && track._processor.onaudioprocess === null) {
        track._processor.onaudioprocess = (e) => {
            // This will be called when the processor needs audio data
            // You would need to queue the samples and feed them here
        };
    }
    
    // Note: This is a simplified version. In reality, you'd need to properly
    // queue and process the audio samples with the Web Audio API
}

export function set_room_event_handler(room, handler) {
    room.on(LivekitClient.RoomEvent.DataReceived, (payload, participant) => {
        handler({
            type: 'data_received',
            payload: payload,
            participant: participant,
        });
    });
    
    room.on(LivekitClient.RoomEvent.TrackSubscribed, (track, publication, participant) => {
        handler({
            type: 'track_subscribed',
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
