syntax = "proto3";

package decentraland.sdk.components;

import "decentraland/common/vectors.proto";
import "decentraland/sdk/components/common/id.proto";

option (common.ecs_component_id) = 1216;

/**
 * This component applies a continuous physics force.

 * @remarks Low-level component. Use Physics.applyForceToPlayer()/.removeForceToPlayer() instead.
 * Direct manipulation will conflict with the force accumulation registry.
 * Summary component: stores the accumulated result of all active forces registered by the scene in the current frame.

 * State-like component: the force is applied every physics tick while the component is present on the entity.
*/
message PBPhysicsCombinedForce {
    decentraland.common.Vector3 vector = 1; // Includes force direction and magnitude
}