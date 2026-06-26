# Decentraland SDK7 Complete Reference Guide

## Table of Contents
1. [Installation & Setup](#installation--setup)
2. [Getting Started](#getting-started)
3. [Architecture & Core Concepts](#architecture--core-concepts)
4. [3D Essentials](#3d-essentials)
5. [Interactivity](#interactivity)
6. [2D UI](#2d-ui)
7. [Blockchain Integration](#blockchain-integration)
8. [Media](#media)
9. [Networking](#networking)
10. [Libraries](#libraries)
11. [Debugging](#debugging)
12. [Programming Patterns](#programming-patterns)
13. [Projects](#projects)
14. [Publishing](#publishing)
15. [Optimization](#optimization)
16. [Design & Experience](#design--experience)
17. [Web Editor](#web-editor)
18. [Advanced Topics](#advanced-topics)

---

## Installation & Setup

### Creator Hub Installation
The Creator Hub is a standalone application for building Decentraland scenes with drag-and-drop interface.

Download: [https://decentraland.org/download/creator-hub](https://decentraland.org/download/creator-hub)

### Code Editor Setup
Install Visual Studio Code: [https://code.visualstudio.com/](https://code.visualstudio.com/)
Alternative: Cursor AI: [https://www.cursor.com/](https://www.cursor.com/)

### CLI Installation
```bash
npm install -g @dcl/sdk-commands
```

### Creating a New Scene
```bash
npx @dcl/sdk-commands init
```

### Basic Imports
```typescript
import { engine } from '@dcl/sdk/ecs'
import { Transform, GltfContainer, MeshRenderer, Material } from '@dcl/sdk/ecs'
import { Vector3, Quaternion, Color4 } from '@dcl/sdk/math'
import { ReactEcsRenderer } from '@dcl/sdk/react-ecs'
```

---

## Getting Started

### SDK Quick Start

#### Basic Scene Structure
```typescript
export function main() {
  // Create entity
  const cube = engine.addEntity()
  
  // Add transform component
  Transform.create(cube, {
    position: Vector3.create(8, 1, 8),
    rotation: Quaternion.Zero(),
    scale: Vector3.create(1, 1, 1)
  })
  
  // Add shape component
  MeshRenderer.setBox(cube)
}
```

#### Adding Materials
```typescript
// Create material
Material.setPbrMaterial(cube, {
  albedoColor: Color4.Red(),
  metallic: 0.8,
  roughness: 0.1
})
```

#### Adding Interactivity
```typescript
import { pointerEventsSystem, InputAction } from '@dcl/sdk/ecs'

pointerEventsSystem.onPointerDown(
  {
    entity: cube,
    opts: { button: InputAction.IA_POINTER, hoverText: 'Click me!' }
  },
  () => {
    console.log('Cube clicked!')
  }
)
```

#### Adding Tweens
```typescript
import { Tween, EasingFunction } from '@dcl/sdk/ecs'

Tween.create(cube, {
  mode: Tween.Mode.Move({
    start: Vector3.create(8, 1, 8),
    end: Vector3.create(12, 1, 8)
  }),
  duration: 2000,
  easingFunction: EasingFunction.EF_LINEAR
})
```

### Development Workflow

#### Preview Scene
```bash
npm run start
```

Preview options:
- `-- --web3`: Connect to browser wallet
- `-- --no-debug`: Disable debug panel
- `-- --explorer-alpha`: Use Decentraland Desktop client
- `-- --port <number>`: Specific port

#### Build Scene
```bash
npm run build
```

#### Deploy Scene
```bash
npm run deploy
```

---

## Architecture & Core Concepts

### Entity-Component-System (ECS)

#### Entities
Entities are basic units - just IDs that group components together.

```typescript
// Create entity
const myEntity = engine.addEntity()

// Remove entity
engine.removeEntity(myEntity)

// Remove entity with children
removeEntityWithChildren(engine, myEntity)
```

#### Components
Components store data about entities.

```typescript
// Create component
Transform.create(myEntity, {
  position: Vector3.create(5, 1, 5)
})

// Get component (read-only)
const transform = Transform.get(myEntity)

// Get mutable component
const mutableTransform = Transform.getMutable(myEntity)
mutableTransform.position.x = 10

// Check if component exists
const hasTransform = Transform.has(myEntity)

// Remove component
Transform.deleteFrom(myEntity)

// Create or replace component
Transform.createOrReplace(myEntity, {
  position: Vector3.create(3, 1, 3)
})
```

#### Systems
Systems contain logic that runs every frame.

```typescript
function mySystem(dt: number) {
  // dt is delta time since last frame
  for (const [entity] of engine.getEntitiesWith(Transform, MeshRenderer)) {
    const transform = Transform.getMutable(entity)
    transform.rotation = Quaternion.multiply(
      transform.rotation,
      Quaternion.fromAngleAxis(dt * 10, Vector3.Up())
    )
  }
}

// Add system to engine
engine.addSystem(mySystem)

// Add system with priority and name
engine.addSystem(mySystem, 1, "RotationSystem")

// Remove system
engine.removeSystem("RotationSystem")
```

#### Querying Components
```typescript
// Query entities with specific components
for (const [entity, transform, meshRenderer] of engine.getEntitiesWith(Transform, MeshRenderer)) {
  // Process entities
}
```

#### Custom Components
```typescript
// Define custom component schema
const HealthSchema = {
  current: Schemas.Number,
  max: Schemas.Number
}

// Create component with default values
const defaultValues = {
  current: 100,
  max: 100
}

export const Health = engine.defineComponent('Health', HealthSchema, defaultValues)

// Use custom component
Health.create(player, { current: 100, max: 100 })

const health = Health.getMutable(player)
health.current -= 10

// Flag components (no data, just marking)
export const IsEnemyFlag = engine.defineComponent('isEnemyFlag', {})
IsEnemyFlag.create(enemy)

// Complex schema types
const ComplexSchema = {
  simpleField: Schemas.Boolean,
  numberList: Schemas.Array(Schemas.Int),
  nestedObject: Schemas.Map({
    nestedField1: Schemas.String,
    nestedField2: Schemas.Vector3
  }),
  enumField: Schemas.EnumString<Color>(Color, Color.Red)
}

// Enum types
enum Color {
  Red = 'red',
  Green = 'green',
  Blue = 'blue'
}

// OneOf types for interchangeable data
const FlexibleSchema = {
  flexField: Schemas.OneOf({ 
    vector: Schemas.Vector3, 
    quaternion: Schemas.Quaternion 
  })
}

// Usage with $case
MyComponent.create(entity, {
  flexField: {
    $case: 'vector',
    value: Vector3.create(1, 2, 3)
  }
})

// Subscribe to component changes
Health.onChange(playerEntity, (healthData) => {
  if (!healthData) return
  console.log('Health changed:', healthData.current)
})
```

#### Entity Relationships
```typescript
// Parent-child relationships
const parent = engine.addEntity()
const child = engine.addEntity()

Transform.create(child, {
  position: Vector3.create(2, 0, 0),
  parent: parent
})

// Get entity by name (from Scene Editor)
const door = engine.getEntityOrNullByName('door-1')
```

#### Reserved Entities
- `engine.PlayerEntity`: Player's avatar
- `engine.CameraEntity`: Player's camera
- `engine.RootEntity`: Scene root

#### Component Change Detection
```typescript
Transform.onChange(myEntity, (newTransform) => {
  if (!newTransform) return
  console.log('Transform changed:', newTransform.position)
})
```

---

## 3D Essentials

### Entity Positioning

#### Transform Component
```typescript
Transform.create(entity, {
  position: Vector3.create(8, 1, 8),    // World position
  rotation: Quaternion.fromEulerDegrees(0, 90, 0), // Rotation
  scale: Vector3.create(2, 2, 2),       // Scale
  parent: parentEntity                   // Optional parent
})
```

#### Position
- Measured in meters
- Scene coordinates: (0,0,0) is South-West corner at ground level
- Single parcel: 16m x 16m
- Scene center: (8, 0, 8) for single parcel

#### Rotation
```typescript
// Using Euler angles (degrees)
const rotation = Quaternion.fromEulerDegrees(0, 90, 0)

// Using quaternion directly
const rotation = Quaternion.create(0, 0.707, 0, 0.707)

// Get Euler angles from quaternion
const eulerAngles = Quaternion.toEuler(rotation)
```

#### Billboard Component
```typescript
// Always face the player
Billboard.create(entity, {
  billboardMode: BillboardMode.BM_Y  // Only rotate on Y axis
})

// Billboard modes
BillboardMode.BM_ALL    // Rotate on all axes
BillboardMode.BM_NONE   // No rotation
BillboardMode.BM_X      // Fixed X axis
BillboardMode.BM_Y      // Fixed Y axis (most common)
BillboardMode.BM_Z      // Fixed Z axis
```

#### Face Target
```typescript
function lookAt(entity: Entity, target: Vector3) {
  const transform = Transform.getMutable(entity)
  const direction = Vector3.subtract(target, transform.position)
  const normalized = Vector3.normalize(direction)
  transform.rotation = Quaternion.lookRotation(normalized)
}
```

#### Avatar Attachment
```typescript
// Attach to player
AvatarAttach.create(entity, {
  anchorPointId: AvatarAnchorPointType.AAPT_RIGHT_HAND
})

// Attach to specific player
AvatarAttach.create(entity, {
  avatarId: '0x123...abc',
  anchorPointId: AvatarAnchorPointType.AAPT_NAME_TAG
})

// Available anchor points
AvatarAnchorPointType.AAPT_HEAD
AvatarAnchorPointType.AAPT_NECK
AvatarAnchorPointType.AAPT_LEFT_HAND
AvatarAnchorPointType.AAPT_RIGHT_HAND
AvatarAnchorPointType.AAPT_NAME_TAG
// ... many more
```

### Shape Components

#### Primitive Shapes
```typescript
// Box
MeshRenderer.setBox(entity)

// Sphere
MeshRenderer.setSphere(entity)

// Plane
MeshRenderer.setPlane(entity)

// Cylinder
MeshRenderer.setCylinder(entity)

// Cone (cylinder with radiusTop = 0)
MeshRenderer.setCylinder(entity, 0, 1)
```

#### 3D Models
```typescript
GltfContainer.create(entity, {
  src: 'models/house.glb'
})

// Check loading state
const loadingState = GltfContainerLoadingState.getOrNull(entity)
if (loadingState?.currentState === LoadingState.FINISHED) {
  // Model loaded
}
```

#### Visibility
```typescript
// Make invisible
VisibilityComponent.create(entity, { visible: false })

// Toggle visibility
const visibility = VisibilityComponent.getMutable(entity)
visibility.visible = !visibility.visible
```

#### UV Mapping
```typescript
// Custom UV coordinates for plane
MeshRenderer.setPlane(entity, [
  0, 0.75,    // Bottom-left
  0.25, 0.75, // Bottom-right
  0.25, 1,    // Top-right
  0, 1        // Top-left
])
```

### Materials

#### PBR Materials
```typescript
Material.setPbrMaterial(entity, {
  albedoColor: Color4.create(1, 0, 0, 1),  // Red
  metallic: 0.8,
  roughness: 0.2,
  emissiveColor: Color4.create(0, 1, 0, 1), // Green glow
  transparencyMode: MaterialTransparencyMode.MTM_ALPHA_BLEND
})
```

#### Basic Materials (Unlit)
```typescript
Material.setBasicMaterial(entity, {
  diffuseColor: Color4.Red()
})
```

#### Textures
```typescript
Material.setPbrMaterial(entity, {
  texture: Material.Texture.Common({
    src: 'assets/textures/wood.png',
    filterMode: TextureFilterMode.TFM_BILINEAR,
    wrapMode: TextureWrapMode.TWM_REPEAT
  })
})
```

#### Multi-layer Textures
```typescript
Material.setPbrMaterial(entity, {
  texture: Material.Texture.Common({ src: 'assets/diffuse.png' }),
  bumpTexture: Material.Texture.Common({ src: 'assets/normal.png' }),
  emissiveTexture: Material.Texture.Common({ src: 'assets/emissive.png' })
})
```

#### Avatar Portraits
```typescript
Material.setPbrMaterial(entity, {
  texture: Material.Texture.Avatar({
    userId: '0x123...abc'
  })
})
```

#### Texture Animation
```typescript
// Animate texture offset
Tween.setTextureMove(entity,
  Vector2.create(0, 0),
  Vector2.create(1, 0),
  2000
)

// Loop texture animation
TweenSequence.create(entity, { sequence: [], loop: TweenLoop.TL_RESTART })
```

#### Transparency
```typescript
// Alpha blend transparency
Material.setPbrMaterial(entity, {
  albedoColor: Color4.create(1, 0, 0, 0.5), // 50% transparent red
  transparencyMode: MaterialTransparencyMode.MTM_ALPHA_BLEND
})

// Alpha test (cutout)
Material.setPbrMaterial(entity, {
  texture: Material.Texture.Common({ src: 'assets/cutout.png' }),
  transparencyMode: MaterialTransparencyMode.MTM_ALPHA_TEST,
  alphaTest: 0.5
})
```

#### Modify GLTF materials
```typescript
import { GltfNodeModifiers, GltfContainer } from '@dcl/sdk/ecs'

// Override the material of an entire GLB
const model = engine.addEntity()
GltfContainer.create(model, { src: 'models/myModel.glb' })
Transform.create(model, { position: Vector3.create(4, 0, 4) })

GltfNodeModifiers.create(model, {
  modifiers: [
    {
      path: '', // empty string = whole model
      material: {
        material: {
          $case: 'pbr',
          pbr: {
            albedoColor: Color4.Red()
          }
        }
      }
    }
  ]
})
```
Tip: set `path` to a specific mesh node to target only that part; use `Material.Texture.Common({ src: '...' })` inside `pbr` to swap textures.

### Move Entities

#### Tween helpers (concise syntax)
```typescript
// Move between two points
Tween.setMove(entity,
  Vector3.create(4, 1, 4),
  Vector3.create(8, 1, 8),
  2000,
  { faceDirection: false, easingFunction: EasingFunction.EF_LINEAR }
)

// Rotate between two rotations
Tween.setRotate(entity,
  Quaternion.fromEulerDegrees(0, 0, 0),
  Quaternion.fromEulerDegrees(0, 180, 0),
  700,
  EasingFunction.EF_EASEOUTBOUNCE
)

// Scale between sizes
Tween.setScale(entity,
  Vector3.create(1, 1, 1),
  Vector3.create(4, 4, 4),
  2000,
  EasingFunction.EF_LINEAR
)

// Continuous movement (meters/second)
Tween.setMoveContinuous(entity, Vector3.create(0, 0, 1), 0.7)

// Continuous rotation (degrees/second)
Tween.setRotateContinuous(entity, Quaternion.fromEulerDegrees(0, -1, 0), 700)
```

#### Tween System
```typescript
// Move between points
Tween.create(entity, {
  mode: Tween.Mode.Move({
    start: Vector3.create(4, 1, 4),
    end: Vector3.create(8, 1, 8)
  }),
  duration: 2000,
  easingFunction: EasingFunction.EF_LINEAR
})

// Rotate
Tween.create(entity, {
  mode: Tween.Mode.Rotate({
    start: Quaternion.fromEulerDegrees(0, 0, 0),
    end: Quaternion.fromEulerDegrees(0, 180, 0)
  }),
  duration: 1000,
  easingFunction: EasingFunction.EF_EASEOUTBOUNCE
})

// Scale
Tween.create(entity, {
  mode: Tween.Mode.Scale({
    start: Vector3.create(1, 1, 1),
    end: Vector3.create(2, 2, 2)
  }),
  duration: 1500,
  easingFunction: EasingFunction.EF_EASEINEXPO
})
```

#### Tween Sequences
```typescript
// Back and forth movement
Tween.create(entity, {
  mode: Tween.Mode.Move({
    start: Vector3.create(4, 1, 4),
    end: Vector3.create(8, 1, 8)
  }),
  duration: 2000,
  easingFunction: EasingFunction.EF_LINEAR
})

TweenSequence.create(entity, {
  sequence: [],
  loop: TweenLoop.TL_YOYO  // Back and forth
})

// Complex sequence
TweenSequence.create(entity, {
  sequence: [
    {
      mode: Tween.Mode.Move({
        start: Vector3.create(8, 1, 8),
        end: Vector3.create(8, 3, 8)
      }),
      duration: 1000,
      easingFunction: EasingFunction.EF_LINEAR
    },
    {
      mode: Tween.Mode.Rotate({
        start: Quaternion.fromEulerDegrees(0, 0, 0),
        end: Quaternion.fromEulerDegrees(0, 360, 0)
      }),
      duration: 1000,
      easingFunction: EasingFunction.EF_LINEAR
    }
  ],
  loop: TweenLoop.TL_RESTART
})
```

#### Tween Control
```typescript
// Pause/resume tween
const tweenData = Tween.getMutable(entity)
tweenData.playing = false  // Pause
tweenData.playing = true   // Resume

// Remove tween
Tween.deleteFrom(entity)
TweenSequence.deleteFrom(entity)

// Detect tween completion
engine.addSystem(() => {
  if (tweenSystem.tweenCompleted(entity)) {
    console.log('Tween finished!')
  }
})
```

#### Manual Movement via Systems
```typescript
// Linear interpolation movement
function moveSystem(dt: number) {
  for (const [entity, moveData] of engine.getEntitiesWith(MoveComponent)) {
    const transform = Transform.getMutable(entity)
    const data = MoveComponent.getMutable(entity)
    
    if (data.fraction < 1) {
      data.fraction += dt * data.speed
      transform.position = Vector3.lerp(data.start, data.end, data.fraction)
    }
  }
}

engine.addSystem(moveSystem)
```

### Colliders

#### Mesh Colliders
```typescript
// Add collider to primitive
MeshCollider.setBox(entity)
MeshCollider.setSphere(entity)
MeshCollider.setPlane(entity)
MeshCollider.setCylinder(entity)

// Custom collision layer
MeshCollider.setBox(entity, ColliderLayer.CL_CUSTOM1)
```

#### GLTF Model Colliders
```typescript
// Use visible geometry as collider
GltfContainer.create(entity, {
  src: 'models/house.glb',
  visibleMeshesCollisionMask: ColliderLayer.CL_PHYSICS,
  invisibleMeshesCollisionMask: ColliderLayer.CL_NONE
})
```

#### Collision Layers
```typescript
// Available collision layers
ColliderLayer.CL_NONE
ColliderLayer.CL_POINTER      // Pointer events
ColliderLayer.CL_PHYSICS      // Player movement blocking
ColliderLayer.CL_PLAYER       // Player avatar body
ColliderLayer.CL_CUSTOM1      // Custom layer 1
ColliderLayer.CL_CUSTOM2      // Custom layer 2
// ... up to CL_CUSTOM8

// Combine layers
const combinedLayers = ColliderLayer.CL_PHYSICS | ColliderLayer.CL_POINTER
```

### Trigger Areas

Detect when the player or any entity enters, stays in, or exits a shaped area. Shapes: box or sphere. Size the area via `Transform.scale`. By default, reacts to the player layer; customize with `ColliderLayer`.

```typescript
import { engine, Transform, TriggerArea, MeshRenderer, MeshCollider, ColliderLayer } from '@dcl/sdk/ecs'
import { Vector3 } from '@dcl/sdk/math'
import { triggerAreaEventsSystem } from '@dcl/sdk/ecs'

// Create a box trigger at (8,0,8), size 4x2x4
const area = engine.addEntity()
TriggerArea.setBox(area) // or TriggerArea.setSphere(area)
Transform.create(area, { position: Vector3.create(8, 0, 8), scale: Vector3.create(4, 2, 4) })

// Optional: visualize area for debugging
MeshRenderer.setBox(area)

// Events
triggerAreaEventsSystem.onTriggerEnter(area, (e) => {
  console.log('Enter by entity', e.trigger.entity)
})
triggerAreaEventsSystem.onTriggerExit(area, () => {
  console.log('Exit')
})
triggerAreaEventsSystem.onTriggerStay(area, () => {
  // Called every frame while inside
})

// Layers: restrict which entities activate the area
TriggerArea.setBox(area, ColliderLayer.CL_CUSTOM1 | ColliderLayer.CL_CUSTOM2)

// Mark a moving entity to activate the area
const mover = engine.addEntity()
Transform.create(mover, { position: Vector3.create(8, 0, 8) })
MeshCollider.setBox(mover, ColliderLayer.CL_CUSTOM1)
```

Result payload (enter/exit/stay callback parameter):
- `triggeredEntity`: area entity id
- `eventType`: ENTER | EXIT | STAY
- `trigger.entity`: entering entity id
- `trigger.layer`, `trigger.position`, `trigger.rotation`, `trigger.scale`

### Sounds

#### Audio Sources
```typescript
// Create audio source
AudioSource.create(entity, {
  audioClipUrl: 'sounds/effect.mp3',
  playing: true,
  loop: false,
  volume: 0.8,
  pitch: 1.0
})

// Control audio
const audio = AudioSource.getMutable(entity)
audio.playing = true
audio.volume = 0.5
```

#### Audio Streaming
```typescript
// Stream audio from URL
AudioStream.create(entity, {
  url: 'https://example.com/stream.mp3',
  playing: true,
  volume: 0.7
})
```

### Text

#### Text Shape
```typescript
TextShape.create(entity, {
  text: 'Hello World!',
  fontSize: 24,
  fontWeight: 'bold',
  color: Color4.White(),
  outlineColor: Color4.Black(),
  outlineWidth: 0.1,
  textAlign: TextAlignMode.TAM_MIDDLE_CENTER
})

// Text alignment options
TextAlignMode.TAM_TOP_LEFT
TextAlignMode.TAM_TOP_CENTER
TextAlignMode.TAM_TOP_RIGHT
TextAlignMode.TAM_MIDDLE_LEFT
TextAlignMode.TAM_MIDDLE_CENTER
TextAlignMode.TAM_MIDDLE_RIGHT
TextAlignMode.TAM_BOTTOM_LEFT
TextAlignMode.TAM_BOTTOM_CENTER
TextAlignMode.TAM_BOTTOM_RIGHT
```

### Camera

#### Camera Control
```typescript
// Get camera mode
const cameraMode = CameraMode.get(engine.CameraEntity)
if (cameraMode.mode === CameraType.CT_FIRST_PERSON) {
  // First person
} else if (cameraMode.mode === CameraType.CT_THIRD_PERSON) {
  // Third person
}

// Virtual camera
VirtualCamera.create(entity, {
  defaultTransition: {
    transitionMode: CameraTransition.CT_SPEED,
    speed: 1.0
  }
})
```

### Animations

#### GLTF Animations
```typescript
// Play animation
Animator.create(entity, {
  states: [
    {
      clip: 'Walk',
      playing: true,
      loop: true,
      speed: 1.0
    }
  ]
})

// Control animation
const animator = Animator.getMutable(entity)
animator.states[0].playing = false
```

### Lights

#### Dynamic Lights
```typescript
import { LightSource } from '@dcl/sdk/ecs'

// Point light
const point = engine.addEntity()
Transform.create(point, { position: Vector3.create(10, 3, 10) })
LightSource.create(point, {
  type: LightSource.Type.Point({}),
  color: Color3.White(),
  intensity: 300 // candela
})

// Spot light with shadows
const spot = engine.addEntity()
Transform.create(spot, {
  position: Vector3.create(8, 4, 8),
  rotation: Quaternion.fromEulerDegrees(-90, 0, 0)
})
LightSource.create(spot, {
  type: LightSource.Type.Spot({ innerAngle: 25, outerAngle: 45 }),
  shadow: true,
  intensity: 800
})

// Toggle a light on/off
const lightData = LightSource.getMutable(point)
lightData.active = !lightData.active

// Limit range (optional)
LightSource.getMutable(point).range = 20

// Light mask (gobo) for spot/point
LightSource.getMutable(spot).shadowMaskTexture = Material.Texture.Common({
  src: 'assets/scene/images/lightmask1.png'
})
```

Notes:
- One active light per parcel maximum; overall lights/shadows are auto-culled based on quality and proximity (up to ~3 shadowed lights visible at once).
- Intensity is in candela; visible distance roughly grows with (sqrt(intensity)).

---

## Interactivity

### User Data

#### Player Position & Rotation
```typescript
function getPlayerData() {
  if (!Transform.has(engine.PlayerEntity)) return
  
  const playerTransform = Transform.get(engine.PlayerEntity)
  const cameraTransform = Transform.get(engine.CameraEntity)
  
  console.log('Player position:', playerTransform.position)
  console.log('Player rotation:', playerTransform.rotation)
  console.log('Camera position:', cameraTransform.position)
  console.log('Camera rotation:', cameraTransform.rotation)
}

engine.addSystem(getPlayerData)
```

#### Get Player Profile
```typescript
import { getPlayer } from '@dcl/sdk/src/players'

function main() {
  const player = getPlayer()
  if (player) {
    console.log('Name:', player.name)
    console.log('User ID:', player.userId)
    console.log('Is Guest:', player.isGuest)
    console.log('Wearables:', player.wearables)
    console.log('Avatar shape:', player.avatar?.bodyShapeUrn)
  }
}
```

#### Get All Players
```typescript
for (const [entity, data, transform] of engine.getEntitiesWith(PlayerIdentityData, Transform)) {
  console.log('Player:', data.address, 'Position:', transform.position)
}
```

#### Camera Mode
```typescript
function checkCameraMode() {
  if (!CameraMode.has(engine.CameraEntity)) return
  
  const cameraMode = CameraMode.get(engine.CameraEntity)
  if (cameraMode.mode === CameraType.CT_FIRST_PERSON) {
    console.log('First person camera')
  } else {
    console.log('Third person camera')
  }
}

engine.addSystem(checkCameraMode)
```

#### Trigger Emotes
```typescript
import { triggerEmote, triggerSceneEmote } from '~system/RestrictedActions'

// Default emote
triggerEmote({ predefinedEmote: 'robot' })

// Custom emote (file must end with _emote.glb)
triggerSceneEmote({ src: 'animations/Snowball_Throw_emote.glb', loop: false })
```
Notes:
- Plays only while the player is still; walking/jumping interrupts.

#### Cursor State
```typescript
// Check if cursor is locked
const isLocked = PointerLock.get(engine.CameraEntity).isPointerLocked

// Get cursor position
const pointerInfo = PrimaryPointerInfo.get(engine.RootEntity)
console.log('Cursor position:', pointerInfo.screenCoordinates)
console.log('Cursor delta:', pointerInfo.screenDelta)
console.log('World ray direction:', pointerInfo.worldRayDirection)
```

### Button Events

#### Click Events
```typescript
// Simple click handler
pointerEventsSystem.onPointerDown(
  {
    entity: myEntity,
    opts: { 
      button: InputAction.IA_POINTER, 
      hoverText: 'Click me!',
      maxDistance: 10
    }
  },
  (event) => {
    console.log('Entity clicked!', event.hit.position)
  }
)

// Multiple button support
pointerEventsSystem.onPointerDown(
  {
    entity: myEntity,
    opts: { 
      button: InputAction.IA_PRIMARY,  // E key
      hoverText: 'Press E'
    }
  },
  () => console.log('E key pressed!')
)

pointerEventsSystem.onPointerDown(
  {
    entity: myEntity,
    opts: { 
      button: InputAction.IA_SECONDARY, // F key
      hoverText: 'Press F'
    }
  },
  () => console.log('F key pressed!')
)
```

#### Available Input Actions
```typescript
InputAction.IA_POINTER    // Left mouse button
InputAction.IA_PRIMARY    // E key
InputAction.IA_SECONDARY  // F key
InputAction.IA_ACTION_3   // 1 key
InputAction.IA_ACTION_4   // 2 key
InputAction.IA_ACTION_5   // 3 key
InputAction.IA_ACTION_6   // 4 key
InputAction.IA_JUMP       // Space key
InputAction.IA_FORWARD    // W key
InputAction.IA_BACKWARD   // S key
InputAction.IA_LEFT       // A key
InputAction.IA_RIGHT      // D key
InputAction.IA_WALK       // Shift key
```

#### System-based Input Events
```typescript
function inputSystem() {
  // Check for specific input on specific entity
  const clickData = inputSystem.getInputCommand(
    InputAction.IA_POINTER,
    PointerEventType.PET_DOWN,
    myEntity
  )
  
  if (clickData) {
    console.log('Entity clicked via system:', clickData.hit.entityId)
  }
  
  // Global input check
  if (inputSystem.isTriggered(InputAction.IA_PRIMARY, PointerEventType.PET_DOWN)) {
    console.log('E key pressed globally')
  }
}

engine.addSystem(inputSystem)
```

#### Event Types
```typescript
PointerEventType.PET_DOWN         // Button pressed
PointerEventType.PET_UP           // Button released
PointerEventType.PET_HOVER_ENTER  // Cursor enters entity
PointerEventType.PET_HOVER_LEAVE  // Cursor leaves entity
```

### Raycasting

#### Basic Raycasting
```typescript
// Raycast from entity in local direction
raycastSystem.registerLocalDirectionRaycast(
  {
    entity: myEntity,
    opts: {
      direction: Vector3.Forward(),
      maxDistance: 10,
      queryType: RaycastQueryType.RQT_HIT_FIRST
    }
  },
  (result) => {
    if (result.hits.length > 0) {
      console.log('Hit entity:', result.hits[0].entityId)
      console.log('Hit position:', result.hits[0].position)
    }
  }
)

// Global direction raycast
raycastSystem.registerGlobalDirectionRaycast(
  {
    entity: myEntity,
    opts: {
      direction: Vector3.Down(),
      maxDistance: 5,
      queryType: RaycastQueryType.RQT_QUERY_ALL
    }
  },
  (result) => {
    console.log('All hits:', result.hits)
  }
)

// Target position raycast
raycastSystem.registerGlobalTargetRaycast(
  {
    entity: myEntity,
    opts: {
      globalTarget: Vector3.create(8, 0, 8),
      maxDistance: 20
    }
  },
  (result) => {
    // Handle result
  }
)

// Target entity raycast
raycastSystem.registerTargetEntityRaycast(
  {
    entity: sourceEntity,
    opts: {
      targetEntity: targetEntity,
      maxDistance: 15
    }
  },
  (result) => {
    // Handle result
  }
)
```

#### Raycast Options
```typescript
const raycastOptions = {
  direction: Vector3.Forward(),
  maxDistance: 16,
  queryType: RaycastQueryType.RQT_HIT_FIRST,  // or RQT_QUERY_ALL
  originOffset: Vector3.create(0, 0.5, 0),   // Offset from entity origin
  collisionMask: ColliderLayer.CL_PHYSICS | ColliderLayer.CL_CUSTOM1,
  continuous: false  // Set to true for continuous raycasting
}
```

#### Collision Layers for Raycasting
```typescript
// Only check specific layers
raycastSystem.registerLocalDirectionRaycast(
  {
    entity: myEntity,
    opts: {
      direction: Vector3.Forward(),
      collisionMask: ColliderLayer.CL_CUSTOM1 | ColliderLayer.CL_CUSTOM2
    }
  },
  (result) => {
    // Only hits entities on CUSTOM1 or CUSTOM2 layers
  }
)
```

#### Remove Raycast
```typescript
// Remove continuous raycast
raycastSystem.removeRaycasterEntity(myEntity)
```

#### Raycast from Player/Camera
```typescript
// Raycast from camera forward
raycastSystem.registerGlobalDirectionRaycast(
  {
    entity: engine.CameraEntity,
    opts: {
      direction: Vector3.rotate(
        Vector3.Forward(),
        Transform.get(engine.CameraEntity).rotation
      ),
      maxDistance: 16
    }
  },
  (result) => {
    if (result.hits.length > 0) {
      console.log('Player looking at:', result.hits[0].entityId)
    }
  }
)
```

### Event Listeners

#### Player Events
```typescript
// Player connects/disconnects
engine.addSystem(() => {
  for (const [entity] of engine.getEntitiesWith(PlayerIdentityData)) {
    // New player joined
    if (!processedPlayers.has(entity)) {
      processedPlayers.add(entity)
      console.log('Player joined:', entity)
    }
  }
})

// Cursor lock/unlock
PointerLock.onChange(engine.CameraEntity, (pointerLock) => {
  if (pointerLock?.isPointerLocked) {
    console.log('Cursor locked')
  } else {
    console.log('Cursor unlocked')
  }
})
```

### Avatar Modifiers

#### Avatar Modifier Areas
```typescript
// Create modifier area
const modifierArea = engine.addEntity()
Transform.create(modifierArea, {
  position: Vector3.create(8, 0, 8),
  scale: Vector3.create(4, 3, 4)  // Area size
})

AvatarModifierArea.create(modifierArea, {
  area: { box: Vector3.create(4, 3, 4) },
  modifiers: [AvatarModifierType.AMT_HIDE_AVATARS],
  excludeIds: ['0x123...abc']  // Optional: exclude specific players
})

// Available modifiers
AvatarModifierType.AMT_HIDE_AVATARS
AvatarModifierType.AMT_DISABLE_PASSPORTS
```

#### Movement Constraints
```typescript
// Create movement constraint area
const constraintArea = engine.addEntity()
Transform.create(constraintArea, {
  position: Vector3.create(8, 0, 8)
})

// Prevent jumping in area
AvatarModifierArea.create(constraintArea, {
  area: { box: Vector3.create(6, 10, 6) },
  modifiers: [AvatarModifierType.AMT_DISABLE_JUMPING]
})
```

### NPC Avatars

#### Display only wearables
```typescript
import { AvatarShape } from '@dcl/sdk/ecs'

const mannequin = engine.addEntity()
AvatarShape.create(mannequin, {
  id: 'npc-1',
  name: 'NPC',
  wearables: [
    'urn:decentraland:matic:collections-v2:0x90e5cb2d673699be8f28d339c818a0b60144c494:0'
  ],
  show_only_wearables: true
})

Transform.create(mannequin, {
  position: Vector3.create(4, 0.25, 5),
  scale: Vector3.create(1.2, 1.2, 1.2)
})
```
Use this to showcase items (e.g., storefront mannequins).

### Input Modifiers
```typescript
import { InputModifier } from '@dcl/sdk/ecs'

// Freeze player
InputModifier.create(engine.PlayerEntity, {
  mode: InputModifier.Mode.Standard({ disableAll: true })
})

// Restrict specific locomotion
InputModifier.createOrReplace(engine.PlayerEntity, {
  mode: InputModifier.Mode.Standard({
    disableRun: true,
    disableJump: true,
    disableEmote: true
  })
})
```
Note: Supported in the DCL 2.0 desktop client; only affects the local player inside scene bounds.

### Move Player

#### Teleport Player
```typescript
// Move player to position
const playerTransform = Transform.getMutable(engine.PlayerEntity)
playerTransform.position = Vector3.create(8, 0, 8)

// Move player with rotation
playerTransform.position = Vector3.create(8, 0, 8)
playerTransform.rotation = Quaternion.fromEulerDegrees(0, 180, 0)
```

#### Restrict Player Movement
```typescript
// System to keep player in bounds
function boundarySystem() {
  const playerTransform = Transform.getMutable(engine.PlayerEntity)
  const pos = playerTransform.position
  
  // Keep within scene bounds
  if (pos.x < 0) playerTransform.position.x = 0
  if (pos.x > 16) playerTransform.position.x = 16
  if (pos.z < 0) playerTransform.position.z = 0
  if (pos.z > 16) playerTransform.position.z = 16
}

engine.addSystem(boundarySystem)
```

### Runtime Data

#### Scene Information
```typescript
import { getRealm } from '~system/Runtime'

executeTask(async () => {
  const realm = await getRealm({})
  console.log('Server:', realm.realmInfo?.serverName)
  console.log('Base URL:', realm.realmInfo?.baseUrl)
})
```

#### Environment Data
```typescript
// Get current time and other runtime info
function runtimeSystem() {
  const time = Date.now()
  const sceneInfo = {
    time: time,
    players: Array.from(engine.getEntitiesWith(PlayerIdentityData)).length
  }
  console.log('Scene info:', sceneInfo)
}

engine.addSystem(runtimeSystem)
```

#### Scene Metadata (getSceneInformation)
```typescript
import { getSceneInformation } from '~system/Runtime'

executeTask(async () => {
  const info = await getSceneInformation({})
  if (!info) return
  const sceneJson = JSON.parse(info.metadataJson)
  console.log(sceneJson.scene?.parcels, sceneJson.spawnPoints)
})
```

### Skybox Control

#### Fixed time of day (scene.json)
```json
"skyboxConfig": {
  "fixedTime": 36000
}
```

#### Read current world time
```typescript
import { getWorldTime } from '~system/Runtime'

executeTask(async () => {
  const time = await getWorldTime({})
  console.log('Seconds since midnight:', time.seconds)
})
```

#### Change time dynamically
```typescript
import { SkyboxTime, TransitionMode } from '~system/Runtime'

// Must target root entity
SkyboxTime.create(engine.RootEntity, { fixed_time: 36000 })

// Optional transition direction
SkyboxTime.createOrReplace(engine.RootEntity, {
  fixed_time: 54000,
  direction: TransitionMode.TM_BACKWARD
})
```

---

## 2D UI

### Basic UI Setup

#### Rendering UI
```typescript
// ui.tsx
import { UiEntity, ReactEcs } from '@dcl/sdk/react-ecs'
import { Color4 } from '@dcl/sdk/math'

export const uiMenu = () => (
  <UiEntity
    uiTransform={{
      width: 400,
      height: 300,
      position: { top: '10%', left: '10%' }
    }}
    uiBackground={{ color: Color4.create(0, 0, 0, 0.8) }}
  >
    <UiEntity
      uiTransform={{
        width: '100%',
        height: 50,
        alignItems: 'center',
        justifyContent: 'center'
      }}
      uiText={{ value: 'Hello World!', fontSize: 24 }}
    />
  </UiEntity>
)

// index.ts
import { ReactEcsRenderer } from '@dcl/sdk/react-ecs'
import { uiMenu } from './ui'

export function main() {
  ReactEcsRenderer.setUiRenderer(uiMenu)
}
```

### UI Transform

#### Positioning
```typescript
// Absolute positioning
uiTransform={{
  positionType: 'absolute',
  position: { top: '10px', left: '20px' },
  width: 200,
  height: 100
}}

// Relative positioning
uiTransform={{
  positionType: 'relative',
  margin: { top: '10px', left: '20px' },
  width: '50%',
  height: '30%'
}}

// Flexbox layout
uiTransform={{
  flexDirection: 'column',        // 'row' or 'column'
  alignItems: 'center',          // 'flex-start', 'center', 'flex-end', 'stretch'
  justifyContent: 'space-between', // 'flex-start', 'center', 'flex-end', 'space-between', 'space-around'
  flexWrap: 'wrap'               // 'nowrap', 'wrap'
}}
```

#### Size and Spacing
```typescript
uiTransform={{
  width: 300,          // Fixed width in pixels
  height: '50%',       // Percentage height
  minWidth: 100,       // Minimum width
  maxWidth: 500,       // Maximum width
  padding: { top: 10, bottom: 10, left: 15, right: 15 },
  margin: { top: '5px', bottom: '5px' }
}}
```

### UI Background

#### Colors and Images
```typescript
// Solid color background
uiBackground={{ color: Color4.create(1, 0, 0, 0.8) }}

// Texture background
uiBackground={{
  texture: { src: 'assets/ui/background.png' },
  textureMode: 'stretch'  // 'stretch', 'center', 'repeat'
}}

// Nine-slice background
uiBackground={{
  texture: { src: 'assets/ui/panel.png' },
  textureSlices: {
    top: 10,
    bottom: 10,
    left: 10,
    right: 10
  }
}}
```

### UI Text

#### Text Properties
```typescript
uiText={{
  value: 'Hello World!',
  fontSize: 18,
  color: Color4.White(),
  textAlign: 'middle-center',  // 'top-left', 'top-center', 'top-right', etc.
  font: 'serif',               // 'sans-serif', 'serif', 'monospace'
  fontWeight: 'bold'           // 'normal', 'bold'
}}

// Rich text with line breaks
uiText={{
  value: 'Line 1\nLine 2\nLine 3',
  fontSize: 16,
  textAlign: 'top-left'
}}
```

### UI Button Events

#### Click Events
```typescript
<UiEntity
  uiTransform={{
    width: 150,
    height: 50,
    alignItems: 'center',
    justifyContent: 'center'
  }}
  uiBackground={{ color: Color4.Blue() }}
  uiText={{ value: 'Click Me!', fontSize: 18 }}
  onMouseDown={() => {
    console.log('Button clicked!')
  }}
/>
```

#### Hover Effects
```typescript
const [isHovered, setIsHovered] = useState(false)

<UiEntity
  uiTransform={{
    width: 150,
    height: 50,
    alignItems: 'center',
    justifyContent: 'center'
  }}
  uiBackground={{ 
    color: isHovered ? Color4.Green() : Color4.Blue() 
  }}
  uiText={{ value: 'Hover Me!', fontSize: 18 }}
  onMouseEnter={() => setIsHovered(true)}
  onMouseLeave={() => setIsHovered(false)}
  onMouseDown={() => {
    console.log('Button clicked!')
  }}
/>
```

### Dynamic UI

#### State Management
```typescript
import { useState } from 'react'

export const DynamicUI = () => {
  const [count, setCount] = useState(0)
  const [isVisible, setIsVisible] = useState(true)

  return (
    <UiEntity
      uiTransform={{
        width: 300,
        height: 200,
        position: { top: '10%', left: '10%' },
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center'
      }}
      uiBackground={{ color: Color4.create(0, 0, 0, 0.8) }}
    >
      {isVisible && (
        <UiEntity
          uiTransform={{ width: '100%', height: 50 }}
          uiText={{ 
            value: `Count: ${count}`, 
            fontSize: 20,
            textAlign: 'middle-center'
          }}
        />
      )}
      
      <UiEntity
        uiTransform={{
          width: 100,
          height: 40,
          margin: { top: 10 }
        }}
        uiBackground={{ color: Color4.Green() }}
        uiText={{ value: '+', fontSize: 24, textAlign: 'middle-center' }}
        onMouseDown={() => setCount(count + 1)}
      />
      
      <UiEntity
        uiTransform={{
          width: 100,
          height: 40,
          margin: { top: 10 }
        }}
        uiBackground={{ color: Color4.Red() }}
        uiText={{ value: 'Toggle', fontSize: 16, textAlign: 'middle-center' }}
        onMouseDown={() => setIsVisible(!isVisible)}
      />
    </UiEntity>
  )
}
```

#### Game HUD Example
```typescript
export const GameHUD = () => {
  const [health, setHealth] = useState(100)
  const [score, setScore] = useState(0)
  const [ammo, setAmmo] = useState(30)

  return (
    <UiEntity
      uiTransform={{
        width: '100%',
        height: '100%',
        positionType: 'absolute'
      }}
    >
      {/* Health Bar */}
      <UiEntity
        uiTransform={{
          width: 200,
          height: 20,
          position: { top: '10px', left: '10px' }
        }}
        uiBackground={{ color: Color4.Red() }}
      >
        <UiEntity
          uiTransform={{
            width: `${health}%`,
            height: '100%'
          }}
          uiBackground={{ color: Color4.Green() }}
        />
      </UiEntity>
      
      {/* Score */}
      <UiEntity
        uiTransform={{
          width: 150,
          height: 30,
          position: { top: '10px', right: '10px' }
        }}
        uiText={{
          value: `Score: ${score}`,
          fontSize: 18,
          textAlign: 'middle-right'
        }}
      />
      
      {/* Ammo */}
      <UiEntity
        uiTransform={{
          width: 100,
          height: 30,
          position: { bottom: '10px', right: '10px' }
        }}
        uiText={{
          value: `Ammo: ${ammo}`,
          fontSize: 16,
          textAlign: 'middle-right'
        }}
      />
    </UiEntity>
  )
}
```

### UI Layout Examples

#### Modal Dialog
```typescript
export const ModalDialog = ({ isOpen, onClose }: { isOpen: boolean, onClose: () => void }) => {
  if (!isOpen) return null

  return (
    <UiEntity
      uiTransform={{
        width: '100%',
        height: '100%',
        positionType: 'absolute',
        alignItems: 'center',
        justifyContent: 'center'
      }}
      uiBackground={{ color: Color4.create(0, 0, 0, 0.5) }}
      onMouseDown={onClose}
    >
      <UiEntity
        uiTransform={{
          width: 400,
          height: 300,
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: { top: 20, bottom: 20, left: 20, right: 20 }
        }}
        uiBackground={{ color: Color4.create(0.2, 0.2, 0.2, 1) }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <UiEntity
          uiText={{
            value: 'Dialog Title',
            fontSize: 24,
            textAlign: 'middle-center'
          }}
        />
        
        <UiEntity
          uiText={{
            value: 'This is the dialog content.',
            fontSize: 16,
            textAlign: 'middle-center'
          }}
        />
        
        <UiEntity
          uiTransform={{
            width: 100,
            height: 40,
            alignItems: 'center',
            justifyContent: 'center'
          }}
          uiBackground={{ color: Color4.Blue() }}
          uiText={{ value: 'Close', fontSize: 16 }}
          onMouseDown={onClose}
        />
      </UiEntity>
    </UiEntity>
  )
}
```

#### Inventory Grid
```typescript
export const InventoryGrid = () => {
  const items = Array.from({ length: 20 }, (_, i) => `Item ${i + 1}`)

  return (
    <UiEntity
      uiTransform={{
        width: 400,
        height: 400,
        position: { top: '10%', left: '10%' },
        flexDirection: 'column',
        padding: { top: 10, bottom: 10, left: 10, right: 10 }
      }}
      uiBackground={{ color: Color4.create(0.1, 0.1, 0.1, 0.9) }}
    >
      <UiEntity
        uiTransform={{
          width: '100%',
          height: 40,
          alignItems: 'center',
          justifyContent: 'center'
        }}
        uiText={{ value: 'Inventory', fontSize: 20 }}
      />
      
      <UiEntity
        uiTransform={{
          width: '100%',
          height: '100%',
          flexDirection: 'row',
          flexWrap: 'wrap',
          alignItems: 'flex-start',
          justifyContent: 'flex-start'
        }}
      >
        {items.map((item, index) => (
          <UiEntity
            key={index}
            uiTransform={{
              width: 70,
              height: 70,
              margin: { top: 5, bottom: 5, left: 5, right: 5 },
              alignItems: 'center',
              justifyContent: 'center'
            }}
            uiBackground={{ color: Color4.create(0.3, 0.3, 0.3, 1) }}
            uiText={{ value: item, fontSize: 10 }}
            onMouseDown={() => console.log(`Clicked ${item}`)}
          />
        ))}
      </UiEntity>
    </UiEntity>
  )
}
```

---

## Blockchain Integration

### Wallet Connection

#### Check Player Wallet
```typescript
import { getPlayer } from '@dcl/sdk/src/players'

function checkWallet() {
  const player = getPlayer()
  if (player && !player.isGuest) {
    console.log('Player wallet address:', player.userId)
  } else {
    console.log('Player is guest (no wallet)')
  }
}
```

### NFT Display

#### Display Certified NFT
```typescript
import { NftShape } from '@dcl/sdk/ecs'

// Display NFT
NftShape.create(entity, {
  urn: 'urn:decentraland:ethereum:erc721:0x06012c8cf97bead5deae237070f9587f8e7a266d:558536',
  color: Color4.White(),
  style: NftFrameType.NFT_CLASSIC
})

// Available frame styles
NftFrameType.NFT_CLASSIC
NftFrameType.NFT_BAROQUE_ORNAMENT
NftFrameType.NFT_DIAMOND_ORNAMENT
NftFrameType.NFT_MINIMAL_WIDE
NftFrameType.NFT_MINIMAL_GREY
NftFrameType.NFT_BLOCKY
NftFrameType.NFT_GOLD_EDGES
NftFrameType.NFT_GOLD_CARVED
NftFrameType.NFT_GOLD_WIDE
NftFrameType.NFT_GOLD_ROUNDED
NftFrameType.NFT_METAL_MEDIUM
NftFrameType.NFT_METAL_WIDE
NftFrameType.NFT_METAL_SLIM
NftFrameType.NFT_METAL_ROUNDED
NftFrameType.NFT_PINS
NftFrameType.NFT_MINIMAL_BLACK
NftFrameType.NFT_MINIMAL_WHITE
NftFrameType.NFT_TAPE
NftFrameType.NFT_WOOD_SLIM
NftFrameType.NFT_WOOD_WIDE
NftFrameType.NFT_WOOD_TWIGS
NftFrameType.NFT_CANVAS
NftFrameType.NFT_NONE
```

### Blockchain Transactions

#### Sign Message
```typescript
import { signedFetch } from '@dcl/sdk/signed-fetch'

executeTask(async () => {
  try {
    const response = await signedFetch('https://example.com/api/action', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        action: 'claimReward',
        amount: 100
      })
    })
    
    const result = await response.json()
    console.log('Transaction result:', result)
  } catch (error) {
    console.log('Transaction failed:', error)
  }
})
```

#### MANA Transactions
```typescript
import { manaUser } from '@dcl/sdk/ethereum'

executeTask(async () => {
  try {
    // Check MANA balance
    const balance = await manaUser.balance()
    console.log('MANA balance:', balance)
    
    // Send MANA
    const result = await manaUser.send('0x123...abc', 100) // 100 MANA
    console.log('MANA sent:', result)
  } catch (error) {
    console.log('MANA transaction failed:', error)
  }
})
```

### Smart Contract Interaction

#### Import Contract ABI
```typescript
// Store ABI in separate file (e.g., contracts/mana.ts)
export default [
  {
    "anonymous": false,
    "inputs": [
      {
        "indexed": true,
        "name": "burner",
        "type": "address"
      },
      {
        "indexed": false,
        "name": "value",
        "type": "uint256"
      }
    ],
    "name": "Burn",
    "type": "event"
  }
  // ... rest of ABI
]

// Import in your scene
import { abi } from '../contracts/mana'
```

#### Create Contract Instance
```typescript
import { RequestManager, ContractFactory } from 'eth-connect'
import { createEthereumProvider } from '@dcl/sdk/ethereum-provider'
import { abi } from '../contracts/mana'

executeTask(async () => {
  try {
    // Create web3 provider interface
    const provider = createEthereumProvider()
    
    // Create request manager for RPC messages
    const requestManager = new RequestManager(provider)
    
    // Create contract factory
    const factory = new ContractFactory(requestManager, abi)
    
    // Instance contract at specific address
    const contract = await factory.at('0x2a8fd99c19271f4f04b1b7b9c4f7cf264b626edb') as any
    
    // Call contract methods
    const result = await contract.balanceOf('0x123...abc')
    console.log('Balance:', result)
    
  } catch (error) {
    console.log('Contract interaction failed:', error)
  }
})
```

#### Gas Price Checking
```typescript
import { RequestManager } from 'eth-connect'
import { createEthereumProvider } from '@dcl/sdk/ethereum-provider'

executeTask(async () => {
  const provider = createEthereumProvider()
  const requestManager = new RequestManager(provider)
  
  // Check current gas price
  const gasPrice = await requestManager.eth_gasPrice()
  console.log('Current gas price:', gasPrice)
  
  // Get account balance
  const balance = await requestManager.eth_getBalance('0x123...abc', 'latest')
  console.log('Account balance:', balance)
})
```

#### Contract Method Calls
```typescript
executeTask(async () => {
  try {
    const userData = getPlayer()
    if (userData.isGuest) return
    
    // Write operation (requires gas)
    const writeResult = await contract.transfer(
      '0xRecipientAddress',
      100, // amount
      {
        from: userData.userId,
        gas: 100000,
        gasPrice: await requestManager.eth_gasPrice()
      }
    )
    console.log('Transaction hash:', writeResult)
    
    // Read operation (no gas required)
    const balance = await contract.balanceOf(userData.userId)
    console.log('Current balance:', balance)
    
  } catch (error) {
    console.log('Transaction failed:', error)
  }
})
```

#### Using Test Networks
```typescript
// For Sepolia testnet testing
// Set Metamask to Sepolia network
// Use test URLs for preview:
// decentraland://realm=http://127.0.0.1:8000&local-scene=true&debug=true&dclenv=zone&position=0,0

// Contract addresses differ between networks
const CONTRACT_ADDRESSES = {
  mainnet: '0x0f5d2fb29fb7d3cfee444a200298f468908cc942',
  sepolia: '0x...' // Test contract address
}

const currentNetwork = 'sepolia' // or determine dynamically
const contractAddress = CONTRACT_ADDRESSES[currentNetwork]
```

---

## Media

### Video Playing

#### Basic Video Setup
```typescript
// Step 1: Create screen entity
const screen = engine.addEntity()
MeshRenderer.setPlane(screen)
Transform.create(screen, { position: Vector3.create(4, 1, 4) })

// Step 2: Create video player
VideoPlayer.create(screen, {
  src: 'videos/myVideo.mp4',  // Local file
  playing: true,
  loop: false,
  volume: 1.0,
  playbackRate: 1.0,
  position: 0  // Start time in seconds
})

// Step 3: Create video texture
const videoTexture = Material.Texture.Video({ videoPlayerEntity: screen })

// Step 4: Apply to material (Basic material recommended)
Material.setBasicMaterial(screen, {
  texture: videoTexture
})
```

#### External Video Streaming
```typescript
// Stream from external URL (must be HTTPS with CORS)
VideoPlayer.create(screen, {
  src: 'https://player.vimeo.com/external/552481870.m3u8?s=c312c8533f97e808fccc92b0510b085c8122a875',
  playing: true
})

// Supported formats: .mp4, .ogg, .webm, .m3u8
```

#### Live Streaming with Decentraland Cast
```typescript
// Use Decentraland's built-in live streaming
VideoPlayer.create(screen, {
  src: 'livekit-video://current-stream',
  playing: true
})

// Requires Admin tools smart item for stream key management
```

#### Video Controls & Events
```typescript
// Interactive video controls
pointerEventsSystem.onPointerDown(
  {
    entity: screen,
    opts: { button: InputAction.IA_POINTER, hoverText: 'Play/Pause' }
  },
  () => {
    const video = VideoPlayer.getMutable(screen)
    video.playing = !video.playing
  }
)

// Stop and rewind
pointerEventsSystem.onPointerDown(
  {
    entity: stopButton,
    opts: { button: InputAction.IA_POINTER, hoverText: 'Stop' }
  },
  () => {
    const video = VideoPlayer.getMutable(screen)
    video.playing = false
    video.position = 0
  }
)

// Video event handling
import { videoEventsSystem, VideoState } from '@dcl/sdk/ecs'

videoEventsSystem.registerVideoEventsEntity(screen, (videoEvent) => {
  console.log('Video state:', videoEvent.state)
  console.log('Current time:', videoEvent.currentOffset)
  console.log('Video length:', videoEvent.videoLength)
  
  switch (videoEvent.state) {
    case VideoState.VS_PLAYING:
      console.log('Video started playing')
      break
    case VideoState.VS_PAUSED:
      console.log('Video paused')
      break
    case VideoState.VS_READY:
      console.log('Video ready to play')
      break
    case VideoState.VS_ERROR:
      console.log('Video error occurred')
      break
  }
})

// Get latest video state
const latestEvent = videoEventsSystem.getVideoState(screen)
if (latestEvent) {
  console.log('Latest state:', latestEvent.state)
}
```

#### Enhanced Video Materials
```typescript
// PBR material with enhanced video appearance
Material.setPbrMaterial(screen, {
  texture: videoTexture,
  roughness: 1.0,
  specularIntensity: 0,
  metallic: 0,
  emissiveTexture: videoTexture,
  emissiveIntensity: 0.6,
  emissiveColor: Color3.White()
})

// Basic material (recommended for performance)
Material.setBasicMaterial(screen, {
  texture: videoTexture
})
```

#### Multiple Video Screens
```typescript
// Share one video across multiple screens
const screen1 = engine.addEntity()
const screen2 = engine.addEntity()

MeshRenderer.setPlane(screen1)
MeshRenderer.setPlane(screen2)
Transform.create(screen1, { position: Vector3.create(4, 1, 4) })
Transform.create(screen2, { position: Vector3.create(6, 1, 4) })

// Only one VideoPlayer component needed
VideoPlayer.create(screen1, {
  src: 'videos/shared-video.mp4',
  playing: true
})

// Same texture applied to both screens
const sharedTexture = Material.Texture.Video({ videoPlayerEntity: screen1 })
Material.setBasicMaterial(screen1, { texture: sharedTexture })
Material.setBasicMaterial(screen2, { texture: sharedTexture })
```

#### Circular Video Screens
```typescript
// Create circular video screen with alpha mask
const videoTexture = Material.Texture.Video({ videoPlayerEntity: screen })
const alphaMask = Material.Texture.Common({
  src: 'assets/circle_mask.png',
  wrapMode: TextureWrapMode.TWM_MIRROR
})

Material.setBasicMaterial(screen, {
  texture: videoTexture,
  alphaTexture: alphaMask
})
```

#### Performance Considerations
```typescript
// Video performance limits:
// - Low quality: 1 simultaneous video
// - Medium quality: 5 simultaneous videos  
// - High quality: 10 simultaneous videos

// Check if video should play based on distance
function videoPerformanceSystem() {
  const playerPos = Transform.get(engine.PlayerEntity).position
  
  for (const [entity, video] of engine.getEntitiesWith(VideoPlayer)) {
    const screenPos = Transform.get(entity).position
    const distance = Vector3.distance(playerPos, screenPos)
    
    const videoMutable = VideoPlayer.getMutable(entity)
    
    // Only play video when player is close
    if (distance < 10 && !videoMutable.playing) {
      videoMutable.playing = true
    } else if (distance > 15 && videoMutable.playing) {
      videoMutable.playing = false
    }
  }
}

engine.addSystem(videoPerformanceSystem)
```

### Audio Streaming

#### Stream Audio
```typescript
AudioStream.create(entity, {
  url: 'https://example.com/stream.mp3',
  playing: true,
  volume: 0.7
})

// Control stream
const stream = AudioStream.getMutable(entity)
stream.playing = false
stream.volume = 0.3
```

---

## Networking

### Network Connections

#### REST API Calls
```typescript
executeTask(async () => {
  try {
    const response = await fetch('https://api.example.com/data')
    const data = await response.json()
    console.log('API response:', data)
  } catch (error) {
    console.log('API call failed:', error)
  }
})
```

#### POST Requests
```typescript
executeTask(async () => {
  try {
    const response = await fetch('https://api.example.com/submit', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        username: 'player123',
        score: 1500
      })
    })
    
    const result = await response.json()
    console.log('Submission result:', result)
  } catch (error) {
    console.log('Submission failed:', error)
  }
})
```

### Multiplayer Sync

#### Synced Entities
```typescript
import { syncEntity } from '@dcl/sdk/network'

// Method 1: Sync predefined entities with explicit IDs
enum EntityIds {
  DOOR = 1,
  ELEVATOR = 2,
  DRAWBRIDGE = 3
}

const door = engine.addEntity()
Transform.create(door, { position: Vector3.create(8, 1, 8) })
MeshRenderer.setBox(door)

// Sync specific components with unique ID
syncEntity(door, [Transform.componentId, MeshRenderer.componentId], EntityIds.DOOR)

// Method 2: Sync player-created entities (auto-assigned ID)
function createProjectile() {
  const projectile = engine.addEntity()
  Transform.create(projectile, { position: Vector3.create(4, 1, 4) })
  MeshRenderer.setSphere(projectile)
  
  // No explicit ID needed for player-created entities
  syncEntity(projectile, [Transform.componentId])
  return projectile
}
```

#### Parent-Child Relationships in Multiplayer
```typescript
import { syncEntity, parentEntity, getParent, getChildren } from '@dcl/sdk/network'

// Create parent and child entities
const parent = engine.addEntity()
const child = engine.addEntity()

// Both must be synced
syncEntity(parent, [Transform.componentId], 1)
syncEntity(child, [Transform.componentId], 2)

// Use parentEntity() instead of Transform.parent for synced entities
parentEntity(child, parent)

// Helper functions
const parentRef = getParent(child)  // Returns parent entity
const childrenArray = Array.from(getChildren(parent))  // Returns [child]

// Remove parent relationship
removeParent(child)  // Child becomes child of root entity
```

#### Check Sync State
```typescript
import { isStateSyncronized } from '@dcl/sdk/network'

function gameStateSystem() {
  const isSynced = isStateSyncronized()
  
  if (isSynced) {
    // Player is synchronized, allow interactions
    enableGameControls()
  } else {
    // Player not synchronized, disable interactions
    disableGameControls()
    showSyncingMessage()
  }
}

engine.addSystem(gameStateSystem)
```

#### Message Bus
```typescript
import { MessageBus } from '@dcl/sdk/message-bus'

const sceneMessageBus = new MessageBus()

// Send messages with payload
sceneMessageBus.emit('player-action', {
  playerId: 'player123',
  action: 'jump',
  timestamp: Date.now(),
  position: Vector3.create(8, 1, 8)
})

// Listen for messages
type PlayerAction = {
  playerId: string
  action: string
  timestamp: number
  position: Vector3
}

sceneMessageBus.on('player-action', (data: PlayerAction) => {
  console.log(`Player ${data.playerId} performed ${data.action}`)
  
  // Handle the action for all players
  handlePlayerAction(data)
})

// Complex multiplayer interaction example
function createMultiplayerCube() {
  const cube = engine.addEntity()
  Transform.create(cube, { position: Vector3.create(8, 1, 8) })
  MeshRenderer.setBox(cube)
  Material.setPbrMaterial(cube, { albedoColor: Color4.Blue() })
  
  syncEntity(cube, [Transform.componentId, Material.componentId], 100)
  
  pointerEventsSystem.onPointerDown(
    {
      entity: cube,
      opts: { button: InputAction.IA_POINTER, hoverText: 'Change Color' }
    },
    () => {
      // Send message to all players about color change
      const newColor = Color4.create(Math.random(), Math.random(), Math.random(), 1)
      
      sceneMessageBus.emit('cube-color-change', {
        cubeId: 100,
        color: newColor,
        timestamp: Date.now()
      })
    }
  )
}

// Handle color change message
sceneMessageBus.on('cube-color-change', (data: any) => {
  // Find the cube and update its color
  for (const [entity] of engine.getEntitiesWith(Transform, Material)) {
    // You'd need to track which entity has which ID
    const material = Material.getMutable(entity)
    material.albedoColor = data.color
  }
})
```

#### Test Multiplayer Locally
```typescript
// Open multiple browser windows to test multiplayer:
// 1. Use Creator Hub Preview button multiple times
// 2. Or use URL: decentraland://realm=http://127.0.0.1:8000&local-scene=true&debug=true

// Track multiple players for testing
function multiplayerTestSystem() {
  const players = Array.from(engine.getEntitiesWith(PlayerIdentityData))
  console.log(`Active players: ${players.length}`)
  
  players.forEach(([entity, playerData]) => {
    const transform = Transform.getOrNull(entity)
    if (transform) {
      console.log(`Player ${playerData.address} at position:`, transform.position)
    }
  })
}

engine.addSystem(multiplayerTestSystem)
```

#### Single Player Mode (Worlds)
```typescript
// For Decentraland Worlds, configure scene.json for single player
/*
{
  "worldConfiguration": {
    "name": "my-world.dcl.eth",
    "fixedAdapter": "offline:offline"
  }
}
*/

// Players won't see each other and no sync is needed
function singlePlayerScene() {
  // No need for syncEntity or MessageBus in offline mode
  const entity = engine.addEntity()
  Transform.create(entity, { position: Vector3.create(8, 1, 8) })
  MeshRenderer.setBox(entity)
  
  // Direct state changes work fine in single player
  pointerEventsSystem.onPointerDown(
    { entity, opts: { button: InputAction.IA_POINTER } },
    () => {
      const transform = Transform.getMutable(entity)
      transform.position.y += 1
    }
  )
}
```

---

## Libraries

### Managing Dependencies

#### Update SDK
```bash
npm install @dcl/sdk@latest
```

#### Add External Libraries
```bash
npm install some-library
```

#### Package.json Example
```json
{
  "dependencies": {
    "@dcl/sdk": "latest"
  },
  "devDependencies": {
    "@dcl/js-runtime": "latest"
  }
}
```

---

## Debugging

### Debug in Preview

#### Console Logging
```typescript
// Basic logging
console.log('Debug message:', data)

// Structured logging
console.log('Entity transform:', {
  entity: entity,
  position: Transform.get(entity).position,
  rotation: Transform.get(entity).rotation
})
```

#### Debug UI Overlay
```typescript
// Debug info display
export const DebugUI = () => {
  const [debugInfo, setDebugInfo] = useState({
    entities: 0,
    fps: 0,
    players: 0
  })

  // Update debug info
  useEffect(() => {
    const interval = setInterval(() => {
      setDebugInfo({
        entities: Array.from(engine.getEntitiesWith(Transform)).length,
        fps: Math.round(1 / engine.deltaTime),
        players: Array.from(engine.getEntitiesWith(PlayerIdentityData)).length
      })
    }, 1000)

    return () => clearInterval(interval)
  }, [])

  return (
    <UiEntity
      uiTransform={{
        width: 200,
        height: 100,
        position: { top: '10px', right: '10px' },
        flexDirection: 'column'
      }}
      uiBackground={{ color: Color4.create(0, 0, 0, 0.7) }}
    >
      <UiEntity uiText={{ value: `Entities: ${debugInfo.entities}`, fontSize: 12 }} />
      <UiEntity uiText={{ value: `FPS: ${debugInfo.fps}`, fontSize: 12 }} />
      <UiEntity uiText={{ value: `Players: ${debugInfo.players}`, fontSize: 12 }} />
    </UiEntity>
  )
}
```

#### Performance Monitoring
```typescript
function performanceSystem(dt: number) {
  if (dt > 0.033) { // More than 30ms per frame
    console.log('Performance warning: Frame time:', dt * 1000, 'ms')
  }
}

engine.addSystem(performanceSystem)
```

### Troubleshooting

#### Common Issues and Solutions

**Entity not visible:**
```typescript
// Check if entity has required components
if (!MeshRenderer.has(entity)) {
  console.log('Entity missing MeshRenderer')
}
if (!Transform.has(entity)) {
  console.log('Entity missing Transform')
}
```

**Click events not working:**
```typescript
// Ensure entity has collider
if (!MeshCollider.has(entity) && !GltfContainer.has(entity)) {
  console.log('Entity needs collider for click events')
  // Use pointer layer so raycasts/clicks hit this entity
  MeshCollider.setBox(entity, ColliderLayer.CL_POINTER)
}
```

**Scene bounds checking:**
```typescript
function checkBounds(entity: Entity) {
  const transform = Transform.get(entity)
  const pos = transform.position
  
  if (pos.x < 0 || pos.x > 16 || pos.z < 0 || pos.z > 16) {
    console.log('Entity outside scene bounds:', pos)
  }
}
```

---

## Programming Patterns

### Async Functions

#### executeTask
```typescript
// Use executeTask for async operations
executeTask(async () => {
  try {
    const data = await fetch('https://api.example.com/data')
    const result = await data.json()
    
    // Use the result in your scene
    updateSceneWithData(result)
  } catch (error) {
    console.log('Async operation failed:', error)
  }
})
```

#### Timers and Delays
```typescript
// Delay execution
executeTask(async () => {
  await new Promise(resolve => setTimeout(resolve, 2000)) // Wait 2 seconds
  console.log('Delayed action executed')
})

// Recurring timer
let timerRunning = true
executeTask(async () => {
  while (timerRunning) {
    await new Promise(resolve => setTimeout(resolve, 1000)) // Wait 1 second
    console.log('Timer tick')
  }
})
```

### Game Objects Pattern

#### Game Object Class
```typescript
class GameObject {
  entity: Entity
  
  constructor(position: Vector3) {
    this.entity = engine.addEntity()
    Transform.create(this.entity, { position })
  }
  
  setPosition(position: Vector3) {
    Transform.getMutable(this.entity).position = position
  }
  
  getPosition(): Vector3 {
    return Transform.get(this.entity).position
  }
  
  destroy() {
    engine.removeEntity(this.entity)
  }
}

class Enemy extends GameObject {
  health: number = 100
  
  constructor(position: Vector3) {
    super(position)
    MeshRenderer.setBox(this.entity)
    Material.setPbrMaterial(this.entity, { albedoColor: Color4.Red() })
  }
  
  takeDamage(amount: number) {
    this.health -= amount
    if (this.health <= 0) {
      this.destroy()
    }
  }
}

// Usage
const enemy = new Enemy(Vector3.create(8, 1, 8))
enemy.takeDamage(50)
```

### Component Factories

#### Reusable Component Creation
```typescript
function createProjectile(start: Vector3, direction: Vector3, speed: number): Entity {
  const projectile = engine.addEntity()
  
  Transform.create(projectile, {
    position: start,
    scale: Vector3.create(0.1, 0.1, 0.1)
  })
  
  MeshRenderer.setSphere(projectile)
  Material.setPbrMaterial(projectile, { 
    albedoColor: Color4.Yellow(),
    emissiveColor: Color4.Yellow()
  })
  
  // Add movement component
  const MovementSchema = {
    velocity: Schemas.Vector3,
    speed: Schemas.Number
  }
  const Movement = engine.defineComponent('Movement', MovementSchema)
  
  Movement.create(projectile, {
    velocity: Vector3.normalize(direction),
    speed: speed
  })
  
  return projectile
}

// Movement system for projectiles
function projectileSystem(dt: number) {
  for (const [entity, movement] of engine.getEntitiesWith(Movement)) {
    const transform = Transform.getMutable(entity)
    const velocity = Vector3.scale(movement.velocity, movement.speed * dt)
    transform.position = Vector3.add(transform.position, velocity)
    
    // Remove if out of bounds
    if (Vector3.length(transform.position) > 50) {
      engine.removeEntity(entity)
    }
  }
}

engine.addSystem(projectileSystem)
```

### State Machines

#### Simple State Machine
```typescript
enum NPCState {
  IDLE = 'idle',
  WALKING = 'walking',
  ATTACKING = 'attacking',
  DEAD = 'dead'
}

const NPCStateSchema = {
  currentState: Schemas.EnumString(NPCState, NPCState.IDLE),
  stateTimer: Schemas.Number
}

const NPCStateMachine = engine.defineComponent('NPCStateMachine', NPCStateSchema)

function npcStateMachineSystem(dt: number) {
  for (const [entity, stateMachine] of engine.getEntitiesWith(NPCStateMachine)) {
    const state = NPCStateMachine.getMutable(entity)
    state.stateTimer += dt
    
    switch (state.currentState) {
      case NPCState.IDLE:
        if (state.stateTimer > 3) {
          state.currentState = NPCState.WALKING
          state.stateTimer = 0
        }
        break
        
      case NPCState.WALKING:
        // Move the NPC
        if (state.stateTimer > 5) {
          state.currentState = NPCState.IDLE
          state.stateTimer = 0
        }
        break
    }
  }
}

engine.addSystem(npcStateMachineSystem)
```

---

## Projects

### Scene Metadata

#### scene.json Configuration
```json
{
  "display": {
    "title": "My Awesome Scene",
    "description": "An amazing Decentraland experience",
    "author": "Your Name",
    "version": "1.0.0",
    "navmapThumbnail": "images/scene-thumbnail.png",
    "favicon": "favicon_asset"
  },
  "contact": {
    "name": "Your Name",
    "email": "your.email@example.com"
  },
  "main": "bin/game.js",
  "tags": ["game", "interactive", "multiplayer"],
  "scene": {
    "parcels": ["0,0"],
    "base": "0,0"
  },
  "spawningPoints": [
    {
      "name": "spawn1",
      "default": true,
      "position": { "x": 8, "y": 0, "z": 8 },
      "cameraTarget": { "x": 8, "y": 1, "z": 12 }
    }
  ],
  "requiredPermissions": [
    "ALLOW_TO_MOVE_PLAYER_INSIDE_SCENE",
    "ALLOW_TO_TRIGGER_AVATAR_EMOTE"
  ]
}
```

#### Multiple Parcels
```json
{
  "scene": {
    "parcels": [
      "0,0", "1,0", "0,1", "1,1"
    ],
    "base": "0,0"
  }
}
```

### Smart Wearables

#### Smart Wearable Setup
```typescript
// Smart wearable entry point
export function main() {
  // Initialize wearable logic
  initializeWearable()
}

function initializeWearable() {
  // Attach effects to player
  const effect = engine.addEntity()
  
  Transform.create(effect, {
    parent: engine.PlayerEntity,
    position: Vector3.create(0, 0.5, 0)
  })
  
  MeshRenderer.setSphere(effect)
  Material.setPbrMaterial(effect, {
    albedoColor: Color4.create(1, 1, 0, 0.5),
    emissiveColor: Color4.Yellow()
  })
}
```

### Portable Experiences

#### Portable Experience Structure
```typescript
// Portable experience that follows player
export function main() {
  createPortableUI()
  
  // Listen for realm changes
  engine.addSystem(() => {
    // Update portable experience based on current realm
  })
}

function createPortableUI() {
  // Create UI that's always available
  ReactEcsRenderer.setUiRenderer(() => (
    <UiEntity
      uiTransform={{
        position: { top: '10px', left: '10px' },
        width: 200,
        height: 50
      }}
      uiBackground={{ color: Color4.create(0, 0, 0, 0.8) }}
      uiText={{ value: 'Portable Experience Active', fontSize: 12 }}
    />
  ))
}
```

---

## Publishing

### Deployment Process

#### Build and Deploy
```bash
# Build the scene
npm run build

# Deploy to Decentraland
npm run deploy
```

#### Deploy to Test Server
```bash
# Deploy to test environment
npm run deploy -- --target-content https://peer-testing.decentraland.org/content
```

#### Deploy to Custom World
```bash
# Deploy to specific world
npm run deploy -- --target worlds-content-server.decentraland.org/world/your-world-name
```

### Publishing Requirements

#### LAND Ownership
- Own LAND tokens
- Have Decentraland NAME
- Have ENS name
- Get permissions from LAND owner

#### Content Validation
- Scene must fit within parcel bounds
- All assets must be under size limits
- No prohibited content
- Performance requirements met

#### Metadata Requirements
- Title and description
- Preview image (scene thumbnail)
- Author information
- Spawn points defined

---

## Optimization

### Performance Guidelines

#### Entity Limits
- Maximum entities per scene: ~10,000
- Maximum polygons: Varies by parcel count
- Texture memory: 64MB per parcel
- Materials: 20 per scene recommended

#### Optimization Techniques
```typescript
// Object pooling for projectiles
class ProjectilePool {
  private pool: Entity[] = []
  private active: Entity[] = []
  
  getProjectile(): Entity {
    if (this.pool.length > 0) {
      const projectile = this.pool.pop()!
      this.active.push(projectile)
      return projectile
    }
    
    return this.createProjectile()
  }
  
  releaseProjectile(projectile: Entity) {
    const index = this.active.indexOf(projectile)
    if (index > -1) {
      this.active.splice(index, 1)
      this.pool.push(projectile)
      
      // Hide the projectile
      Transform.getMutable(projectile).position = Vector3.create(0, -100, 0)
    }
  }
  
  private createProjectile(): Entity {
    const projectile = engine.addEntity()
    MeshRenderer.setSphere(projectile)
    Material.setPbrMaterial(projectile, { albedoColor: Color4.Yellow() })
    this.active.push(projectile)
    return projectile
  }
}
```

#### LOD (Level of Detail)
```typescript
function lodSystem() {
  const playerPos = Transform.get(engine.PlayerEntity).position
  
  for (const [entity, transform] of engine.getEntitiesWith(Transform, MeshRenderer)) {
    const distance = Vector3.distance(playerPos, transform.position)
    
    if (distance > 30) {
      // Far: hide entity
      VisibilityComponent.createOrReplace(entity, { visible: false })
    } else if (distance > 15) {
      // Medium: show simple version
      VisibilityComponent.createOrReplace(entity, { visible: true })
      // Could switch to lower poly model here
    } else {
      // Close: show full detail
      VisibilityComponent.createOrReplace(entity, { visible: true })
    }
  }
}

engine.addSystem(lodSystem)
```

#### Texture Optimization
```typescript
// Use compressed texture formats
Material.setPbrMaterial(entity, {
  texture: Material.Texture.Common({
    src: 'assets/compressed_texture.webp', // Use WebP instead of PNG
    filterMode: TextureFilterMode.TFM_TRILINEAR
  })
})

// Share textures between materials
const sharedTexture = Material.Texture.Common({
  src: 'assets/shared_texture.webp'
})

Material.setPbrMaterial(entity1, { texture: sharedTexture })
Material.setPbrMaterial(entity2, { texture: sharedTexture })
```

### Scene Limitations

#### Parcel-based Limits
- 1 parcel: 16m x 16m area, ~20m height
- More parcels = higher height limit
- Materials: 20 per scene recommended
- Textures: 512x512 recommended, 1024x1024 max

#### Performance Targets
- 30 FPS minimum
- <100ms system execution per frame
- Reasonable memory usage

---

## Design & Experience

### UX Guidelines

#### Player Onboarding
```typescript
// Welcome sequence
function createWelcomeSequence() {
  // Show welcome message
  ui.displayAnnouncement('Welcome to the scene!')
  
  // Highlight interactive objects
  for (const [entity] of engine.getEntitiesWith(PointerEvents)) {
    addGlowEffect(entity)
  }
  
  // Remove highlights after delay
  setTimeout(() => {
    for (const [entity] of engine.getEntitiesWith(GlowEffect)) {
      GlowEffect.deleteFrom(entity)
    }
  }, 10000)
}
```

#### Clear Visual Feedback
```typescript
// Hover feedback for interactive objects
pointerEventsSystem.onPointerDown(
  {
    entity: button,
    opts: { 
      button: InputAction.IA_POINTER, 
      hoverText: 'Click to activate',
      maxDistance: 8,
      showFeedback: true  // Shows outline when hovered
    }
  },
  () => {
    // Provide immediate feedback
    ui.displayAnnouncement('Activated!')
    
    // Visual feedback
    const transform = Transform.getMutable(button)
    Tween.create(button, {
      mode: Tween.Mode.Scale({
        start: transform.scale,
        end: Vector3.scale(transform.scale, 1.2)
      }),
      duration: 200,
      easingFunction: EasingFunction.EF_EASEOUTBOUNCE
    })
  }
)
```

#### Accessibility Considerations
```typescript
// Text legibility
TextShape.create(entity, {
  text: 'Important Information',
  fontSize: 24,
  color: Color4.White(),
  outlineColor: Color4.Black(),
  outlineWidth: 0.1  // Improves readability
})

// Audio cues
function playAudioCue(sound: string) {
  const audioEntity = engine.addEntity()
  AudioSource.create(audioEntity, {
    audioClipUrl: `sounds/${sound}.mp3`,
    playing: true,
    volume: 0.8
  })
  
  // Clean up after playing
  setTimeout(() => {
    engine.removeEntity(audioEntity)
  }, 3000)
}
```

### Game Design Patterns

#### Quest System
```typescript
interface Quest {
  id: string
  title: string
  description: string
  objectives: QuestObjective[]
  completed: boolean
}

interface QuestObjective {
  id: string
  description: string
  completed: boolean
}

class QuestManager {
  private quests: Map<string, Quest> = new Map()
  
  addQuest(quest: Quest) {
    this.quests.set(quest.id, quest)
  }
  
  completeObjective(questId: string, objectiveId: string) {
    const quest = this.quests.get(questId)
    if (!quest) return
    
    const objective = quest.objectives.find(o => o.id === objectiveId)
    if (objective) {
      objective.completed = true
      
      // Check if all objectives completed
      if (quest.objectives.every(o => o.completed)) {
        quest.completed = true
        this.onQuestCompleted(quest)
      }
    }
  }
  
  private onQuestCompleted(quest: Quest) {
    ui.displayAnnouncement(`Quest completed: ${quest.title}`)
    // Award rewards, etc.
  }
}
```

#### Inventory System
```typescript
interface InventoryItem {
  id: string
  name: string
  icon: string
  quantity: number
}

class Inventory {
  private items: Map<string, InventoryItem> = new Map()
  private maxSlots: number = 20
  
  addItem(itemId: string, quantity: number = 1): boolean {
    if (this.items.size >= this.maxSlots && !this.items.has(itemId)) {
      return false // Inventory full
    }
    
    const existingItem = this.items.get(itemId)
    if (existingItem) {
      existingItem.quantity += quantity
    } else {
      // Add new item (would need item definition lookup)
      this.items.set(itemId, {
        id: itemId,
        name: 'Item Name',
        icon: 'item_icon.png',
        quantity: quantity
      })
    }
    
    this.updateInventoryUI()
    return true
  }
  
  removeItem(itemId: string, quantity: number = 1): boolean {
    const item = this.items.get(itemId)
    if (!item || item.quantity < quantity) {
      return false
    }
    
    item.quantity -= quantity
    if (item.quantity <= 0) {
      this.items.delete(itemId)
    }
    
    this.updateInventoryUI()
    return true
  }
  
  private updateInventoryUI() {
    // Update UI to reflect inventory changes
  }
}
```

---

## Web Editor

### Smart Items

#### Creating Smart Items
```typescript
// Smart Item definition
export interface SmartItemProps {
  enabled: boolean
  clickText: string
  onActivate?: () => void
}

export function SmartButton({ enabled, clickText, onActivate }: SmartItemProps) {
  const entity = engine.addEntity()
  
  MeshRenderer.setBox(entity)
  Material.setPbrMaterial(entity, {
    albedoColor: enabled ? Color4.Green() : Color4.Gray()
  })
  
  if (enabled && onActivate) {
    pointerEventsSystem.onPointerDown(
      {
        entity: entity,
        opts: { button: InputAction.IA_POINTER, hoverText: clickText }
      },
      onActivate
    )
  }
  
  return entity
}
```

#### Smart Item Actions
```typescript
// Available actions that can be triggered
export enum SmartItemAction {
  ACTIVATE = 'activate',
  DEACTIVATE = 'deactivate',
  TOGGLE = 'toggle',
  MOVE_TO = 'moveTo',
  ROTATE_TO = 'rotateTo',
  SCALE_TO = 'scaleTo',
  CHANGE_COLOR = 'changeColor',
  PLAY_SOUND = 'playSound',
  SHOW_TEXT = 'showText'
}

// Action implementation
export function executeAction(entity: Entity, action: SmartItemAction, parameters: any) {
  switch (action) {
    case SmartItemAction.MOVE_TO:
      Tween.create(entity, {
        mode: Tween.Mode.Move({
          start: Transform.get(entity).position,
          end: parameters.position
        }),
        duration: parameters.duration || 2000,
        easingFunction: EasingFunction.EF_LINEAR
      })
      break
      
    case SmartItemAction.CHANGE_COLOR:
      Material.setPbrMaterial(entity, {
        albedoColor: parameters.color
      })
      break
      
    case SmartItemAction.PLAY_SOUND:
      AudioSource.createOrReplace(entity, {
        audioClipUrl: parameters.soundUrl,
        playing: true,
        volume: parameters.volume || 1.0
      })
      break
  }
}
```

### Combine Scene Editor with Code

Link your scene code to entities created and configured via the Creator Hub.

#### Reference entities by name
```typescript
import { EntityNames } from '../assets/scene/entity-names'

export function main() {
  // Get by enum (generated by Creator Hub)
  const door1 = engine.getEntityOrNullByName(EntityNames.Door_1)

  // Get by string name (as shown in the Scene Editor tree)
  const door2 = engine.getEntityOrNullByName('Door 2')

  if (door1 && door2) {
    pointerEventsSystem.onPointerDown(
      { entity: door1, opts: { button: InputAction.IA_PRIMARY, hoverText: 'Open' } },
      () => {
        // custom logic
      }
    )
  }
}
```

Validate existence at compile-time with a generic:
```typescript
import { EntityNames } from '../assets/scene/entity-names'

const door = engine.getEntityByName<EntityNames>(EntityNames.Door_1)
// No null-check needed
console.log(Transform.get(door).position.x)
```

Only reference by name inside `main()`, systems, or functions called after `main()` to ensure entities are instantiated.

#### Iterate named entities and fetch children
```typescript
import { Name } from '@dcl/sdk/ecs'

// Iterate all named entities
for (const [entity, name] of engine.getEntitiesWith(Name)) {
  console.log({ entity, name })
}

// Helper to get all children of a parent entity
function getChildren(parent: Entity): Entity[] {
  const childEntities: Entity[] = []
  for (const [entity, transform] of engine.getEntitiesWith(Transform)) {
    if (transform.parent === parent) childEntities.push(entity)
  }
  return childEntities
}
```

#### Fetch entities by tag
```typescript
import { engine } from '@dcl/sdk/ecs'

export function main() {
  const tagged = engine.getEntitiesByTag('myTag')
  for (const entity of tagged) {
    // Handle each tagged entity
  }
}
```

Add or remove tags from code:
```typescript
import { Tags } from '@dcl/sdk/ecs'

Tags.add(entity, 'myTag')
Tags.remove(entity, 'myTag')
```

#### Smart item triggers (Creator Hub asset-packs)
```typescript
import { getTriggerEvents } from '@dcl/asset-packs/dist/events'
import { TriggerType } from '@dcl/asset-packs'
import { EntityNames } from '../assets/scene/entity-names'

export function main() {
  const restart = engine.getEntityOrNullByName(EntityNames.Restart_Button)
  if (restart) {
    const triggers = getTriggerEvents(restart)
    triggers.on(TriggerType.ON_CLICK, () => {
      // restartGame()
    })
  }
}
```

#### Smart item actions (listen and emit)
```typescript
import { getTriggerEvents, getActionEvents } from '@dcl/asset-packs/dist/events'
import { TriggerType } from '@dcl/asset-packs'
import { EntityNames } from '../assets/scene/entity-names'

export function main() {
  const button = engine.getEntityOrNullByName(EntityNames.Red_Button)
  const door = engine.getEntityOrNullByName(EntityNames.Wooden_Door)
  if (button && door) {
    const buttonTriggers = getTriggerEvents(button)
    const doorActions = getActionEvents(door)

    // Listen to actions
    doorActions.on('Open', () => {
      console.log('Door opened!')
    })

    // Emit an action when button is triggered
    buttonTriggers.on(TriggerType.ON_INPUT_ACTION, () => {
      doorActions.emit('Open', {})
    })
  }
}
```

#### Read other smart item components
```typescript
import { getComponents } from '@dcl/asset-packs'
import { getTriggerEvents } from '@dcl/asset-packs/dist/events'
import { TriggerType } from '@dcl/asset-packs'
import { EntityNames } from '../assets/scene/entity-names'

export function main() {
  const chest = engine.getEntityOrNullByName(EntityNames.chest)
  if (chest) {
    const chestTriggers = getTriggerEvents(chest)
    chestTriggers.on(TriggerType.ON_INPUT_ACTION, () => {
      const { States } = getComponents(engine)
      const current = States.getMutableOrNull(chest)?.currentValue
      console.log('chest new state', current)
    })
  }
}
```

---

## Advanced Topics

### Custom Shaders (Not Currently Supported)
*Note: Custom shaders are not currently supported in Decentraland SDK7, but this section provides context for future features.*

### Physics Simulation
*Note: Advanced physics beyond basic colliders are not currently available in SDK7.*

### WebAssembly Integration
*Note: WASM support is limited in the current SDK7 implementation.*

### Scene Analytics

#### Basic Analytics
```typescript
// Track player interactions
function trackInteraction(action: string, object: string) {
  console.log(`Analytics: ${action} on ${object}`)
  
  // Send to analytics service
  executeTask(async () => {
    try {
      await fetch('https://analytics.example.com/track', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          action: action,
          object: object,
          timestamp: Date.now(),
          scene: 'my-scene-id'
        })
      })
    } catch (error) {
      console.log('Analytics tracking failed:', error)
    }
  })
}

// Usage
pointerEventsSystem.onPointerDown(
  { entity: button, opts: { button: InputAction.IA_POINTER } },
  () => {
    trackInteraction('click', 'main-button')
  }
)
```

### Scene Boundaries and Validation

#### Boundary Checking
```typescript
function boundaryCheckSystem() {
  for (const [entity, transform] of engine.getEntitiesWith(Transform)) {
    const pos = transform.position
    
    // Check if entity is outside scene bounds
    if (pos.x < 0 || pos.x > 16 || pos.z < 0 || pos.z > 16 || pos.y > 20) {
      console.log('Entity outside bounds:', entity, pos)
      
      // Optionally move back to bounds
      const mutableTransform = Transform.getMutable(entity)
      mutableTransform.position = Vector3.create(
        Math.max(0, Math.min(16, pos.x)),
        Math.max(0, Math.min(20, pos.y)),
        Math.max(0, Math.min(16, pos.z))
      )
    }
  }
}

engine.addSystem(boundaryCheckSystem)
```

---

## Common Patterns and Best Practices

### Entity Management
```typescript
// Entity factory pattern
class EntityFactory {
  static createPlayer(position: Vector3): Entity {
    const player = engine.addEntity()
    Transform.create(player, { position })
    MeshRenderer.setBox(player)
    Material.setPbrMaterial(player, { albedoColor: Color4.Blue() })
    return player
  }
  
  static createPickup(position: Vector3, type: string): Entity {
    const pickup = engine.addEntity()
    Transform.create(pickup, { position })
    MeshRenderer.setSphere(pickup)
    
    // Add pickup component
    const PickupSchema = { type: Schemas.String }
    const Pickup = engine.defineComponent('Pickup', PickupSchema)
    Pickup.create(pickup, { type })
    
    return pickup
  }
}
```

### Component Composition
```typescript
// Composable behavior components
const HealthSchema = { current: Schemas.Number, max: Schemas.Number }
const MovementSchema = { speed: Schemas.Number, direction: Schemas.Vector3 }
const AISchema = { state: Schemas.String, target: Schemas.Entity }

const Health = engine.defineComponent('Health', HealthSchema)
const Movement = engine.defineComponent('Movement', MovementSchema)
const AI = engine.defineComponent('AI', AISchema)

// Create different entity types by combining components
function createPlayer(position: Vector3) {
  const player = engine.addEntity()
  Transform.create(player, { position })
  Health.create(player, { current: 100, max: 100 })
  Movement.create(player, { speed: 5, direction: Vector3.Zero() })
  // No AI component - player controlled
}

function createEnemy(position: Vector3) {
  const enemy = engine.addEntity()
  Transform.create(enemy, { position })
  Health.create(enemy, { current: 50, max: 50 })
  Movement.create(enemy, { speed: 2, direction: Vector3.Zero() })
  AI.create(enemy, { state: 'patrol', target: 0 as Entity })
}
```

### Error Handling
```typescript
// Safe component access
function safeGetTransform(entity: Entity): Vector3 | null {
  try {
    if (Transform.has(entity)) {
      return Transform.get(entity).position
    }
    return null
  } catch (error) {
    console.log('Error getting transform:', error)
    return null
  }
}

// Graceful degradation
function attemptAction(entity: Entity, action: () => void) {
  try {
    action()
  } catch (error) {
    console.log('Action failed:', error)
    // Provide fallback behavior
    ui.displayAnnouncement('Action temporarily unavailable')
  }
}
```

This comprehensive reference covers all major aspects of Decentraland SDK7 development, from basic setup to advanced patterns and optimization techniques. Use this as a complete reference for building scenes, implementing interactivity, managing assets, and creating engaging experiences in Decentraland. 
