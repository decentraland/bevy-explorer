syntax = "proto3";

package decentraland.sdk.components;

import "decentraland/common/vectors.proto";
import "decentraland/sdk/components/common/id.proto";

option (common.ecs_component_id) = 1215;

/**
 * This component applies a one-shot physics summary impulse.
 
 * @remarks Low-level component. Use Physics.applyImpulseToPlayer() instead.
 * Direct manipulation will conflict with the force accumulation registry.
 * Summary component: stores the accumulated result of all impulses registered by the scene in the current frame.
 
 * Event-like component: each new impulse must increment the eventID to ensure delivery via CRDT, even if the direction is identical to the previous one.
 * Renderer processes impulse with the unique ID only once. Increase eventID of the component to apply another impulse.
*/
message PBPhysicsCombinedImpulse {
    decentraland.common.Vector3 vector = 1; // Includes impulse direction and magnitude
    uint32 event_id = 2; // Monotonic counter to distinguish different impulses.
}