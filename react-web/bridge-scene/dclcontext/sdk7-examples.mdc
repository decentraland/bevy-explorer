# Decentraland SDK 7 Scenes Context7 Reference

This reference documents common patterns, components, and systems used in Decentraland SDK 7 scenes based on example implementations.

## Table of Contents
- @Scene Structure
- @Entity-Component System
- @Component Reference
- @UI System
- @Runtime Data
- @Player Data & Camera Controls
- @Input Handling
- @Event Listeners
- @Movement & Animation
- @Lights & Visual Effects
- @Triggers & Interactions
- @Scene Optimization
- @Restricted Actions
- @Testing Framework
- @Network Connections
- @Blockchain Operations

## Scene Structure

### Basic Project Structure
```
├── src/
│   ├── index.ts          # Main entry point
│   ├── components.ts     # Custom component definitions
│   ├── systems.ts        # Custom system implementations
│   ├── factory.ts        # Entity creation functions
│   ├── utils.ts          # Helper functions
│   └── ui.tsx            # UI definitions with React
├── package.json
└── tsconfig.json
```

### Main Entry Point
```typescript
// index.ts
import { engine } from '@dcl/sdk/ecs'
import { setupUi } from './ui'
import { mySystem } from './systems'

export function main() {
  // Add systems to the engine
  engine.addSystem(mySystem)
  
  // Initialize UI
  setupUi()
  
  // Create initial entities
  // ...
}
```

## Entity-Component System

### Creating Entities
```typescript
import { engine, Transform, MeshRenderer, MeshCollider } from '@dcl/sdk/ecs'
import { Vector3 } from '@dcl/sdk/math'

// Create a new entity
const entity = engine.addEntity()

// Add components to the entity
Transform.create(entity, { 
  position: Vector3.create(8, 1, 8),
  scale: Vector3.create(1, 1, 1)
})

// Add a visual mesh
MeshRenderer.setBox(entity) // Predefined shapes: setBox, setSphere, setPlane, etc.

// Add collision
MeshCollider.setBox(entity)
```

### Defining Custom Components
```typescript
// components.ts
import { Schemas, engine } from '@dcl/sdk/ecs'

// Define a component with properties
export const Spinner = engine.defineComponent('spinner', { 
  speed: Schemas.Number 
})

// Define a tag component (no properties)
export const Cube = engine.defineComponent('cube-id', {})
```

### Creating Systems
```typescript
// systems.ts
import { engine, Transform } from '@dcl/sdk/ecs'
import { Quaternion, Vector3 } from '@dcl/sdk/math'
import { Spinner } from './components'

// System that rotates entities with the Spinner component
export function circularSystem(dt: number) {
  // Query all entities with both Spinner and Transform components
  const entitiesWithSpinner = engine.getEntitiesWith(Spinner, Transform)
  
  for (const [entity, spinner, transform] of entitiesWithSpinner) {
    // Get a mutable reference to modify the component
    const mutableTransform = Transform.getMutable(entity)
    
    // Apply rotation based on the spinner speed and delta time
    mutableTransform.rotation = Quaternion.multiply(
      mutableTransform.rotation,
      Quaternion.fromAngleAxis(dt * spinner.speed, Vector3.Up())
    )
  }
}
```

## Component Reference

### Transform
```typescript
// Position, rotation, and scale
Transform.create(entity, {
  position: Vector3.create(x, y, z),
  rotation: Quaternion.fromEulerDegrees(x, y, z), // or Quaternion.create()
  scale: Vector3.create(x, y, z),
  parent: parentEntity // optional, for hierarchical transformations
})

// Update transform
const transform = Transform.getMutable(entity)
transform.position = Vector3.create(newX, newY, newZ)
```

### Mesh Rendering
```typescript
// Basic shapes
MeshRenderer.setBox(entity)
MeshRenderer.setSphere(entity)
MeshRenderer.setPlane(entity)

// Material
import { Material, MeshRenderer } from '@dcl/sdk/ecs'
import { Color4 } from '@dcl/sdk/math'

// PBR material
Material.setPbrMaterial(entity, {
  albedoColor: Color4.fromHexString("#FF0000"),
  metallic: 0.5,
  roughness: 0.5,
  // other properties: emissiveColor, reflectivityColor, etc.
})

// Basic material
Material.setBasicMaterial(entity, {
  diffuseColor: Color4.White()
})
```

### 3D Models
```typescript
import { GltfContainer } from '@dcl/sdk/ecs'

// Load a 3D model
GltfContainer.create(entity, {
  src: 'models/model.glb',
  visibleMeshesCollisionMask: ColliderLayer.CL_POINTER // Optional
})

// Check loading state
if (GltfContainerLoadingState.get(entity).currentState === LoadingState.FINISHED) {
  // Model is loaded
}
```

### Colliders
```typescript
import { MeshCollider, ColliderLayer } from '@dcl/sdk/ecs'

// Basic colliders
MeshCollider.setBox(entity)
MeshCollider.setSphere(entity)
MeshCollider.setPlane(entity)

// With specific collision layer
MeshCollider.setBox(entity, ColliderLayer.CL_PHYSICS)
```

### Text
```typescript
import { TextShape } from '@dcl/sdk/ecs'
import { Color4 } from '@dcl/sdk/math'

TextShape.create(entity, {
  text: 'Hello Decentraland',
  fontSize: 3,
  textColor: Color4.White(),
  outlineWidth: 0.1,
  outlineColor: Color4.Black(),
  width: 4,
  height: 2,
  textWrapping: true
})
```

### Billboard
```typescript
import { Billboard } from '@dcl/sdk/ecs'

// Makes an entity always face the camera
Billboard.create(entity)
```

### Audio
```typescript
import { AudioSource } from '@dcl/sdk/ecs'

AudioSource.create(entity, {
  audioClipUrl: 'sounds/mySound.mp3',
  playing: true,
  loop: false,
  volume: 1.0
})

// Play sound
AudioSource.getMutable(entity).playing = true
```

## UI System

### Setting Up React UI
```typescript
// ui.tsx
import ReactEcs, { ReactEcsRenderer, UiEntity, Label, Button } from '@dcl/sdk/react-ecs'
import { Color4 } from '@dcl/sdk/math'

export function setupUi() {
  ReactEcsRenderer.setUiRenderer(uiComponent)
}

const uiComponent = () => (
  <UiEntity
    uiTransform={{
      width: 400,
      height: 230,
      margin: '16px 0 8px 270px',
      padding: 4
    }}
    uiBackground={{ color: Color4.create(0.5, 0.8, 0.1, 0.6) }}
  >
    <Label
      value="Hello Decentraland"
      color={Color4.White()}
      fontSize={24}
    />
    <Button
      value="Click Me"
      variant="primary"
      fontSize={14}
      onMouseDown={() => {
        console.log('Button clicked')
      }}
    />
  </UiEntity>
)
```

### UI Components

#### UiEntity
```typescript
<UiEntity
  uiTransform={{
    width: 400,                  // Pixels or percentage (e.g. '100%')
    height: 300,
    position: { top: 10, left: 10 }, // For absolute positioning
    positionType: 'absolute',    // 'absolute' or 'relative'
    display: 'flex',             // 'flex' or 'none'
    flexDirection: 'column',     // 'column' or 'row'
    alignItems: 'center',        // 'center', 'flex-start', 'flex-end'
    justifyContent: 'center',    // 'center', 'flex-start', 'flex-end', 'space-between'
    margin: 5,                   // Or { top: 5, right: 10, bottom: 5, left: 10 }
    padding: 5                   // Same as margin
  }}
  uiBackground={{
    color: Color4.White(),
    texture: { src: 'images/image.png' },
    textureMode: 'stretch',      // 'stretch', 'nine-slices', 'center'
    avatarTexture: { userId: 'user-id' } // For rendering avatar
  }}
/>
```

#### Label
```typescript
<Label
  value="Text content"
  color={Color4.Black()}
  fontSize={18}
  textAlign="middle-center" // 'top-left', 'middle-right', etc.
  font="serif"              // 'serif', 'monospace', or default sans-serif
  uiTransform={{ width: 200, height: 50 }}
/>
```

#### Button
```typescript
<Button
  value="Click Me"
  variant="primary"        // 'primary', 'secondary', etc.
  fontSize={14}
  color={Color4.White()}   // Text color
  uiTransform={{ width: 100, height: 40 }}
  onMouseDown={() => { /* action */ }}
  uiBackground={{ color: Color4.Blue() }} // Override default button style
/>
```

#### Input
```typescript
<Input
  placeholder="Enter text..."
  placeholderColor={Color4.Gray()}
  color={Color4.Black()}    // Text color
  fontSize={16}
  onChange={(value) => { console.log('Value changing: ' + value) }}
  onSubmit={(value) => { console.log('Submitted: ' + value) }}
  uiTransform={{ width: 200, height: 40 }}
/>
```

#### Dropdown
```typescript
<Dropdown
  options={['Option 1', 'Option 2', 'Option 3']}
  onChange={(index) => { console.log('Selected option: ' + index) }}
  fontSize={16}
  color={Color4.Black()}
  uiTransform={{ width: 200, height: 40 }}
  acceptEmpty={true}
  emptyLabel="-- Select an option --"
/>
```

### Canvas Information
```typescript
import { UiCanvasInformation, engine } from '@dcl/sdk/ecs'

// Get screen info
const canvasInfo = UiCanvasInformation.get(engine.RootEntity)
const screenWidth = canvasInfo.width
const screenHeight = canvasInfo.height
const pixelRatio = canvasInfo.devicePixelRatio
```

## Runtime Data

### World Time
```typescript
import { getWorldTime } from '~system/Runtime'

// Get the current time in the Decentraland world
executeTask(async () => {
  const time = await getWorldTime({})
  console.log(`Current time: ${time.seconds} seconds since day start`)
  
  // Convert to hours (24-hour cycle)
  const hours = (time.seconds / 3600) % 24
  console.log(`Current hour: ${hours.toFixed(2)}`)
  
  // Check if it's night time (between 19:50 and 6:15)
  const isNight = time.seconds > 19.833 * 3600 || time.seconds < 6.25 * 3600
  if (isNight) {
    console.log('It is night time')
  }
})
```

### Realm Information
```typescript
import { getRealm } from '~system/Runtime'

// Get information about the current realm
executeTask(async () => {
  const { realmInfo } = await getRealm({})
  console.log(`Current realm: ${realmInfo.realmName}`)
  console.log(`Network ID: ${realmInfo.networkId}`)
  console.log(`Is preview: ${realmInfo.isPreview}`)
  
  // Check if connected to scene room
  if (realmInfo.isConnectedSceneRoom) {
    console.log('Connected to scene room')
  }
})
```

### Platform Detection
```typescript
import { getPlatform } from '~system/EnvironmentApi'

// Detect the platform the player is using
executeTask(async () => {
  const { platform } = await getPlatform()
  
  if (platform === 'BROWSER') {
    console.log('Running in browser')
    // Optimize for browser performance
  } else if (platform === 'DESKTOP') {
    console.log('Running in desktop app')
    // Enable higher quality features
  }
})
```

### Engine Information
```typescript
import { EngineInfo, engine } from '@dcl/sdk/ecs'

// Access engine information
engine.addSystem((deltaTime) => {
  const engineInfo = EngineInfo.getOrNull(engine.RootEntity)
  if (!engineInfo) return
  
  // Get current frame number
  const currentFrame = engineInfo.frameNumber
  
  // Get total runtime in seconds
  const runtime = engineInfo.totalRuntime
  
  // Get current tick number
  const currentTick = engineInfo.tickNumber
  
  // Example: Log every 100 frames
  if (currentFrame % 100 === 0) {
    console.log(`Runtime: ${runtime.toFixed(2)}s, Frame: ${currentFrame}, Tick: ${currentTick}`)
  }
})
```

## Player Data & Camera Controls

### Player Position and Rotation
```typescript
import { engine, Transform } from '@dcl/sdk/ecs'
import { Vector3, Quaternion } from '@dcl/sdk/math'

// Get player position and rotation
function getPlayerPosition() {
  if (!Transform.has(engine.PlayerEntity)) return
  if (!Transform.has(engine.CameraEntity)) return

  // Player position (at chest height, ~0.88m above ground)
  const playerPos = Transform.get(engine.PlayerEntity).position

  // Player rotation (direction the avatar is facing)
  const playerRot = Transform.get(engine.PlayerEntity).rotation

  // Camera position (at eye level, ~1.75m above ground in 1st person)
  const cameraPos = Transform.get(engine.CameraEntity).position

  // Camera rotation
  const cameraRot = Transform.get(engine.CameraEntity).rotation

  console.log('Player position:', playerPos)
  console.log('Player rotation:', playerRot)
  console.log('Camera position:', cameraPos)
  console.log('Camera rotation:', cameraRot)
}

// Add as a system to continuously track player position
engine.addSystem(getPlayerPosition)

// Get player position once
executeTask(async () => {
  // Wait for player entity to be available
  await new Promise(resolve => setTimeout(resolve, 1000))
  getPlayerPosition()
})
```

### Player Identity Data
```typescript
import { engine, PlayerIdentityData, AvatarBase, AvatarEquippedData } from '@dcl/sdk/ecs'

// Access player identity and avatar data
function getPlayerData() {
  for (const [entity, identity, base, equipped] of engine.getEntitiesWith(
    PlayerIdentityData,
    AvatarBase,
    AvatarEquippedData
  )) {
    // Player address and guest status
    console.log('Player address:', identity.address)
    
    // Avatar base information
    console.log('Player name:', base.name)
    console.log('Body shape:', base.bodyShapeUrn)
    console.log('Skin color:', base.skinColor)
    console.log('Eye color:', base.eyeColor)
    console.log('Hair color:', base.hairColor)
    
    // Equipped wearables and emotes
    console.log('Wearables:', equipped.wearableUrns)
    console.log('Emotes:', equipped.emoteUrns)
  }
}

// Add as a system to continuously track player data
engine.addSystem(getPlayerData)
```

### Get Player Information
```typescript
import { getPlayer } from '@dcl/sdk/network'

// Get current player information
executeTask(async () => {
  const player = getPlayer()
  
  console.log('Player ID:', player.userId)
  console.log('Player address:', player.publicKey)
  console.log('Player name:', player.name)
  console.log('Player description:', player.description)
  console.log('Player avatar:', player.avatar)
  
  // Access avatar details
  console.log('Body shape:', player.avatar.bodyShape)
  console.log('Wearables:', player.avatar.wearables)
  console.log('Emotes:', player.avatar.emotes)
})
```

### Get Portable Experiences
```typescript
import { getPortableExperiencesLoaded } from '~system/PortableExperiences'

// Check if player has portable experiences loaded
executeTask(async () => {
  const { loaded } = await getPortableExperiencesLoaded({})
  
  if (loaded.length > 0) {
    console.log('Player has portable experiences loaded:')
    loaded.forEach(px => {
      console.log(`- ID: ${px.id}`)
    })
  } else {
    console.log('Player has no portable experiences loaded')
  }
})
```

### Camera Mode
```typescript
import { engine, CameraMode, CameraType } from '@dcl/sdk/ecs'

// Check player's camera mode (1st or 3rd person)
function checkCameraMode() {
  if (!CameraMode.has(engine.CameraEntity)) return
  
  const cameraMode = CameraMode.get(engine.CameraEntity)
  
  if (cameraMode.mode === CameraType.CT_THIRD_PERSON) {
    console.log('Player is using 3rd person camera')
  } else {
    console.log('Player is using 1st person camera')
  }
}

// Add as a system to continuously check camera mode
engine.addSystem(checkCameraMode)

// Create a camera mode area to force first-person view
import { CameraModeArea } from '@dcl/sdk/ecs'

function createFirstPersonArea() {
  const area = engine.addEntity()
  
  CameraModeArea.create(area, {
    area: Vector3.create(5, 5, 5),  // Box size
    mode: CameraType.CT_FIRST_PERSON
  })
  
  Transform.create(area, {
    position: Vector3.create(8, 1, 8)
  })
}
```

### Pointer Lock
```typescript
import { engine, PointerLock } from '@dcl/sdk/ecs'

// Check if the player's cursor is locked
function checkPointerLock() {
  if (!PointerLock.has(engine.CameraEntity)) return
  
  const isLocked = PointerLock.get(engine.CameraEntity).isPointerLocked
  
  if (isLocked) {
    console.log('Cursor is locked (camera control mode)')
  } else {
    console.log('Cursor is unlocked (UI interaction mode)')
  }
}

// Add as a system to continuously check pointer lock
engine.addSystem(checkPointerLock)

// Listen for pointer lock changes
import { inputSystem, InputAction, PointerEventType } from '@dcl/sdk/ecs'

function setupPointerLockListener() {
  // Check for right mouse button press (unlocks cursor)
  if (inputSystem.isTriggered(InputAction.IA_SECONDARY, PointerEventType.PET_DOWN)) {
    console.log('Right mouse button pressed - cursor unlocked')
  }
  
  // Check for left mouse button press (locks cursor)
  if (inputSystem.isTriggered(InputAction.IA_PRIMARY, PointerEventType.PET_DOWN)) {
    console.log('Left mouse button pressed - cursor locked')
  }
}

engine.addSystem(setupPointerLockListener)
```

## Input Handling

### Pointer Events
```typescript
import { PointerEvents, PointerEventType, InputAction, pointerEventsSystem } from '@dcl/sdk/ecs'

// Add clickable behavior to an entity
PointerEvents.create(entity, {
  pointerEvents: [
    { 
      eventType: PointerEventType.PET_DOWN, 
      eventInfo: { 
        button: InputAction.IA_POINTER,
        hoverText: 'Click me',
        showFeedback: true,       // Show interaction feedback
        maxDistance: 10           // Max interaction distance 
      } 
    }
  ]
})

// Response to click events
pointerEventsSystem.onPointerDown(
  { entity, opts: { button: InputAction.IA_POINTER } },
  (event) => {
    console.log('Entity clicked!')
    // Handle the click
  }
)
```

### Input System
```typescript
import { inputSystem, InputAction, PointerEventType } from '@dcl/sdk/ecs'

// Check if a key/button is pressed
if (inputSystem.isPressed(InputAction.IA_FORWARD)) {
  // W key or forward movement is active
}

// Check for a single press/trigger
if (inputSystem.isTriggered(InputAction.IA_JUMP, PointerEventType.PET_DOWN)) {
  // Space bar was just pressed
}

// Check for key release
if (inputSystem.isTriggered(InputAction.IA_PRIMARY, PointerEventType.PET_UP)) {
  // Primary button was just released
}
```

### Input Modifiers
```typescript
import { InputModifier } from '@dcl/sdk/ecs'

// Disable player movement controls
InputModifier.create(engine.PlayerEntity, {
  mode: {
    $case: 'standard',
    standard: {
      disableWalk: true,  // Disable walking
      disableRun: true,   // Disable running
      disableJump: true   // Disable jumping
    }
  }
})

// Re-enable movement
InputModifier.getMutable(engine.PlayerEntity).mode = {
  $case: 'standard',
  standard: {
    disableWalk: false,
    disableRun: false,
    disableJump: false
  }
}
```

## Event Listeners

### Scene Entry/Exit Events
```typescript
import { onEnterScene, onLeaveScene } from '@dcl/sdk/src/players'

// Listen for players entering the scene
onEnterScene((player) => {
  if (!player) return
  console.log('Player entered:', player.userId)
  console.log('Player data:', player)
})

// Listen for players leaving the scene
onLeaveScene((userId) => {
  if (!userId) return
  console.log('Player left:', userId)
})

// Filter for current player only
import { getPlayer } from '@dcl/sdk/network'

export function main() {
  let myPlayer = getPlayer()

  onEnterScene((player) => {
    if (!player) return
    if (myPlayer && player.userId == myPlayer.userId) {
      console.log('I entered the scene')
    }
  })

  onLeaveScene((userId) => {
    if (!userId) return
    if (myPlayer && userId == myPlayer.userId) {
      console.log('I left the scene')
    }
  })
}
```

### Avatar Emote Events
```typescript
import { AvatarEmoteCommand } from '@dcl/sdk/ecs'

// Listen for emote events from the current player
AvatarEmoteCommand.onChange(engine.PlayerEntity, (emote) => {
  if (!emote) return
  console.log('Emote played:', emote.emoteUrn)
  console.log('Is looping:', emote.loop)
  console.log('Timestamp:', emote.timestamp)
})

// Listen for emote events from other players
function setupEmoteListener(playerEntity: Entity) {
  AvatarEmoteCommand.onChange(playerEntity, (emote) => {
    if (!emote) return
    console.log('Player emote:', emote.emoteUrn)
  })
}
```

### Camera Mode Events
```typescript
import { CameraMode } from '@dcl/sdk/ecs'

// Listen for camera mode changes
CameraMode.onChange(engine.CameraEntity, (cameraComponent) => {
  if (!cameraComponent) return
  console.log('Camera mode:', cameraComponent.mode)
  // 0 = first person
  // 1 = third person
})
```

### Pointer Lock Events
```typescript
import { PointerLock } from '@dcl/sdk/ecs'

// Listen for cursor lock state changes
PointerLock.onChange(engine.CameraEntity, (pointerLock) => {
  if (!pointerLock) return
  console.log('Cursor locked:', pointerLock.isPointerLocked)
})
```

### Avatar Profile Events
```typescript
import { AvatarEquippedData, AvatarBase } from '@dcl/sdk/ecs'

// Listen for wearable/emote changes
AvatarEquippedData.onChange(engine.PlayerEntity, (equipped) => {
  if (!equipped) return
  console.log('Wearables:', equipped.wearableUrns)
  console.log('Emotes:', equipped.emoteUrns)
})

// Listen for avatar base changes
AvatarBase.onChange(engine.PlayerEntity, (body) => {
  if (!body) return
  console.log('Name:', body.name)
  console.log('Body shape:', body.bodyShapeUrn)
  console.log('Skin color:', body.skinColor)
  console.log('Eye color:', body.eyeColor)
  console.log('Hair color:', body.hairColor)
})
```

## Movement & Animation

### Tweens
```typescript
import { Tween, EasingFunction, TweenSequence, TweenLoop } from '@dcl/sdk/ecs'
import { Vector3, Quaternion } from '@dcl/sdk/math'

// Move an entity
Tween.setMove(entity, 
  Vector3.create(1, 0, 1), 
  Vector3.create(5, 0, 5), 
  2000,
  EasingFunction.EF_LINEAR
)

// Rotate an entity
Tween.setRotate(entity, 
  Quaternion.fromEulerDegrees(0, 0, 0), 
  Quaternion.fromEulerDegrees(0, 180, 0), 
  2000,
  EasingFunction.EF_EASEINQUAD
)

// Scale an entity
Tween.setScale(entity, 
  Vector3.create(1, 1, 1), 
  Vector3.create(2, 2, 2), 
  2000,
  EasingFunction.EF_EASEOUTQUAD
})

// Move continuously
Tween.setMoveContinuous(entity, 
  Vector3.create(0, 0, 1), 
  2000
)

// Rotate continuously
Tween.setRotateContinuous(entity, 
  Quaternion.fromEulerDegrees(0, 0, 90), 
  2000
)

// Tween sequences (chained animations)
TweenSequence.create(entity, {
  sequence: [
    {
      mode: Tween.Mode.Move({
        start: Vector3.create(5, 0, 5),
        end: Vector3.create(10, 0, 5)
      }),
      duration: 2000,
      easingFunction: EasingFunction.EF_LINEAR
    },
    {
      mode: Tween.Mode.Move({
        start: Vector3.create(10, 0, 5),
        end: Vector3.create(10, 0, 10)
      }),
      duration: 2000,
      easingFunction: EasingFunction.EF_LINEAR
    }
  ],
  loop: TweenLoop.TL_RESTART  // Can be TL_RESTART, TL_YOYO, or undefined (no loop)
})

// Texture move
Tween.setTextureMove(entity, 
  Vector2.create(0, 0), 
  Vector2.create(1, 0), 
  2000
)

// Texture move continuously
Tween.setTextureMoveContinuous(entity, 
  Vector2.create(0, 1), 
  2000
)


// Control tween playback
const tween = Tween.getMutable(entity)
tween.playing = false  // Pause the tween
tween.currentTime = 0  // Reset to beginning
```

### Animator Component
```typescript
import { Animator, engine } from '@dcl/sdk/ecs'

// Create an entity with a 3D model
const shark = engine.addEntity()
GltfContainer.create(shark, {
  src: 'models/shark.glb'
})

// Add the Animator component with animation states
Animator.create(shark, {
  states: [
    {
      clip: 'swim',  // Name of the animation clip in the model
      playing: true,
      loop: true,
      speed: 1.0,
      weight: 1.0
    },
    {
      clip: 'bite',
      playing: false,
      loop: false,
      speed: 1.0,
      weight: 0.0
    }
  ]
})

// Play a specific animation
Animator.playSingleAnimation(shark, 'swim')

// Stop all animations
Animator.stopAllAnimations(shark)

// Get a specific animation clip to modify its properties
const swimAnim = Animator.getClip(shark, 'swim')
if (swimAnim) {
  swimAnim.speed = 0.5  // Play at half speed
  swimAnim.weight = 0.8  // Set animation weight
}

// Play multiple animations with different weights
Animator.create(shark, {
  states: [
    {
      clip: 'swim',
      playing: true,
      loop: true,
      weight: 0.7
    },
    {
      clip: 'bite',
      playing: true,
      loop: false,
      weight: 0.3
    }
  ]
})

// Create an animation that resets to the first frame when finished
Animator.create(shark, {
  states: [
    {
      clip: 'bite',
      playing: true,
      loop: false,
      shouldReset: true  // Return to first frame when animation ends
    }
  ]
})
```

### Moving the Player
```typescript
import { movePlayerTo } from '~system/RestrictedActions'

// Move the player to a position in the scene
movePlayerTo({ 
  newRelativePosition: { x: 8, y: 0, z: 8 },
  cameraTarget: { x: 10, y: 1, z: 8 }  // Optional: where to look at
})
```

### Avatar Shapes
```typescript
import { AvatarShape } from '@dcl/sdk/ecs'

// Create an NPC avatar
AvatarShape.create(entity, {
  id: 'npc-id',
  name: 'NPC Name',
  bodyShape: 'urn:decentraland:off-chain:base-avatars:BaseMale',  // or BaseFemale
  wearables: [
    'urn:decentraland:off-chain:base-avatars:eyebrows_00',
    'urn:decentraland:off-chain:base-avatars:mouth_00',
    'urn:decentraland:off-chain:base-avatars:eyes_00',
    'urn:decentraland:off-chain:base-avatars:blue_tshirt',
    'urn:decentraland:off-chain:base-avatars:brown_pants',
    'urn:decentraland:off-chain:base-avatars:classic_shoes',
    'urn:decentraland:off-chain:base-avatars:short_hair'
  ],
  hairColor: { r: 0.92, g: 0.76, b: 0.62 },    // RGB values 0-1
  skinColor: { r: 0.94, g: 0.85, b: 0.6 },     // RGB values 0-1
  emotes: []
})
```

### Camera Control
```typescript
import { MainCamera, VirtualCamera, CameraModeArea, CameraType } from '@dcl/sdk/ecs'

// Create a virtual camera
VirtualCamera.create(entity, {
  lookAtEntity: targetEntity,  // Optional: entity to focus on
  defaultTransition: { 
    transitionMode: VirtualCamera.Transition.Time(2)  // 2 second transition
    // Or VirtualCamera.Transition.Speed(10)  // Speed-based transition
  }
})

// Activate a virtual camera
MainCamera.getMutable(engine.CameraEntity).virtualCameraEntity = cameraEntity

// Return to normal camera
MainCamera.getMutable(engine.CameraEntity).virtualCameraEntity = undefined

// Create a camera mode area to force first-person view
CameraModeArea.create(entity, {
  area: Vector3.create(5, 5, 5),  // Box size
  mode: CameraType.CT_FIRST_PERSON  // Or CT_THIRD_PERSON
})
```

### Emotes
```typescript
import { triggerEmote, triggerSceneEmote } from '~system/RestrictedActions'

// Play a predefined avatar emote
triggerEmote({ predefinedEmote: 'robot' })  // 'wave', 'dance', etc.

// Play a custom animation
triggerSceneEmote({ 
  src: 'animations/myAnimation.glb',
  loop: false
})
```

## Lights & Visual Effects

### Lights
```typescript
import { LightSource, PBLightSource_ShadowType } from '@dcl/sdk/ecs'
import { Color3 } from '@dcl/sdk/math'

// Create a point light
LightSource.create(entity, {
  color: Color3.White(),
  intensity: 1.0,
  range: 10,
  active: true,
  type: LightSource.Type.Point({
    shadow: PBLightSource_ShadowType.ST_HARD  // ST_HARD, ST_SOFT, or ST_NONE
  })
})

// Create a spotlight
LightSource.create(entity, {
  color: Color3.Yellow(),
  intensity: 1.5,
  range: 15,
  active: true,
  type: LightSource.Type.Spot({
    innerAngle: 30,   // Inner cone angle in degrees
    outerAngle: 60,   // Outer cone angle in degrees
    shadow: PBLightSource_ShadowType.ST_HARD,
    shadowMaskTexture: Material.Texture.Common({ src: 'textures/mask.png' })  // Optional light mask
  })
})
```

### Visibility Control
```typescript
import { VisibilityComponent } from '@dcl/sdk/ecs'

// Hide an entity
VisibilityComponent.create(entity, { visible: false })

// Show it again
VisibilityComponent.getMutable(entity).visible = true
```

## Triggers & Interactions

### Raycasting
```typescript
import { Raycast, RaycastQueryType, raycastSystem, RaycastResult } from '@dcl/sdk/ecs'
import { Vector3 } from '@dcl/sdk/math'
import { ColliderLayer } from '@dcl/sdk/ecs'

// Basic raycast from entity
Raycast.create(entity, {
  originOffset: Vector3.Zero(),  // Offset from entity position
  direction: { $case: 'globalDirection', globalDirection: Vector3.Down() },
  maxDistance: 10,
  queryType: RaycastQueryType.RQT_HIT_FIRST,  // First hit
  timestamp: Date.now()  // Used to identify this raycast
})

// Direction types for raycasts
// 1. Global direction (ignores entity rotation)
direction: { $case: 'globalDirection', globalDirection: Vector3.Down() }

// 2. Local direction (relative to entity's forward direction)
direction: { $case: 'localDirection', localDirection: Vector3.Forward() }

// 3. Global target (points to a specific world position)
direction: { $case: 'globalTarget', globalTarget: Vector3.create(10, 0, 10) }

// 4. Target entity (points to another entity)
direction: { $case: 'targetEntity', targetEntity: targetEntityId }

// Query types
queryType: RaycastQueryType.RQT_HIT_FIRST  // Returns only the first hit
queryType: RaycastQueryType.RQT_QUERY_ALL  // Returns all hits along the ray

// Global raycast with callback
raycastSystem.registerGlobalDirectionRaycast(
  {
    entity: engine.PlayerEntity,
    opts: { 
      direction: Vector3.Down(),
      maxDistance: 10,
      collisionMask: ColliderLayer.CL_PHYSICS  // Only hit specific layers
    }
  },
  (raycastResult) => {
    if (raycastResult.hits.length > 0) {
      console.log('Hit at', raycastResult.hits[0].position)
    }
  }
)

// Access raycast results in a system
engine.addSystem(() => {
  for (const [entity, result] of engine.getEntitiesWith(RaycastResult)) {
    if (result.hits.length > 0) {
      // Process hits
      for (const hit of result.hits) {
        console.log(`Hit entity: ${hit.entityId}`)
        console.log(`Hit position: ${hit.position}`)
        console.log(`Hit normal: ${hit.normalHit}`)
        console.log(`Hit distance: ${hit.length}`)
      }
    }
  }
})

// Raycast from camera
raycastSystem.registerGlobalDirectionRaycast(
  {
    entity: engine.CameraEntity,
    opts: {
      queryType: RaycastQueryType.RQT_HIT_FIRST,
      direction: Vector3.rotate(
        Vector3.Forward(),
        Transform.get(engine.CameraEntity).rotation
      ),
    },
  },
  function (raycastResult) {
    console.log(raycastResult)
  }
)

// Continuous raycast (runs every frame)
Raycast.create(entity, {
  direction: { $case: 'localDirection', localDirection: Vector3.Forward() },
  maxDistance: 16,
  queryType: RaycastQueryType.RQT_HIT_FIRST,
  originOffset: Vector3.create(0.5, 0, 0), // Prevent self-collision
  continuous: true // Run every frame
})

// Raycast between two entities
Raycast.create(entity1, {
  direction: {
    $case: "targetEntity",
    targetEntity: entity2
  },
  maxDistance: 16,
  queryType: RaycastQueryType.RQT_QUERY_ALL
})
```

### Avatar Modifier Areas
```typescript
import { AvatarModifierArea, AvatarModifierType } from '@dcl/sdk/ecs'

// Create an area that hides other avatars
AvatarModifierArea.create(entity, {
  area: Vector3.create(5, 5, 5),  // Box size
  modifiers: [AvatarModifierType.AMT_HIDE_AVATARS],
  // Or AMT_DISABLE_PASSPORTS
  excludeIds: ['user-address-1', 'user-address-2']  // Optional: players not affected
})
```

### Portable Experiences
```typescript
import { spawn, kill, SpawnResponse } from '~system/PortableExperiences'

// Launch a portable experience
let pxId: SpawnResponse
spawn({ ens: 'experience.dcl.eth' }).then((response) => {
  pxId = response
})

// Close a portable experience
if (pxId?.pid) {
  kill({ pid: pxId.pid })
}
```

## Restricted Actions

### External Links
```typescript
import { openExternalUrl, openNftDialog } from '~system/RestrictedActions'

// Open a webpage
openExternalUrl({ url: 'https://decentraland.org' })

// Open NFT info dialog
openNftDialog({ 
  urn: 'urn:decentraland:ethereum:erc721:0x06012c8cf97bead5deae237070f9587f8e7a266d:1540722' 
})
```

### Teleportation
```typescript
import { teleportTo, changeRealm } from '~system/RestrictedActions'

// Teleport to another scene
teleportTo({ worldCoordinates: { x: 10, y: 20 } })

// Change Decentraland realm
changeRealm({ 
  realm: 'https://peer.decentraland.org',
  message: 'Do you want to change realms?' // Optional confirmation message
})
```

## Testing Framework

### Writing Tests
```typescript
import { test } from '@dcl/sdk/testing'
import { assertComponentValue } from '@dcl/sdk/testing/assert'

test('my test case', function* (context) {
  // Create test setup
  const entity = engine.addEntity()
  Transform.create(entity, { position: Vector3.One() })
  
  // Let the engine run for a frame
  yield
  
  // Check component values
  assertComponentValue(entity, Transform, {
    position: Vector3.One(),
    scale: Vector3.One(),
    rotation: Quaternion.Identity()
  })
})
```

### Test Assertions
```typescript
import { assertEquals, assertEntitiesCount } from '@dcl/sdk/testing/assert'

// Basic assertions
assertEquals(actual, expected, 'Optional message')

// Entity assertions
assertEntitiesCount(engine.getEntitiesWith(MeshRenderer), 5, 'Should have 5 entities with MeshRenderer')
```

## Scene Optimization

### Entity Pooling
```typescript
// Create an entity pool for reuse
const entityPool: Entity[] = []

function getEntityFromPool(): Entity {
  if (entityPool.length > 0) {
    return entityPool.pop()!
  } else {
    return createNewEntity()
  }
}

function returnEntityToPool(entity: Entity) {
  // Reset the entity to a clean state
  VisibilityComponent.getMutable(entity).visible = false
  entityPool.push(entity)
}
```

### Visibility Culling
```typescript
// Create a system that hides distant entities
engine.addSystem(() => {
  const playerPos = Transform.get(engine.PlayerEntity).position
  
  for (const [entity, transform] of engine.getEntitiesWith(Transform, VisibilityComponent)) {
    const distance = Vector3.distance(playerPos, transform.position)
    
    if (distance > 20) {
      VisibilityComponent.getMutable(entity).visible = false
    } else {
      VisibilityComponent.getMutable(entity).visible = true
    }
  }
})
```

## Network Connections

### Fetch API
```typescript
import { executeTask } from '@dcl/sdk/ecs'

// Basic GET request
executeTask(async () => {
  try {
    const response = await fetch('https://api.example.com/data')
    const json = await response.json()
    console.log('Response:', json)
  } catch (error) {
    console.error('Failed to fetch:', error)
  }
})

// POST request with headers and body
executeTask(async () => {
  try {
    const response = await fetch('https://api.example.com/data', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        key: 'value'
      })
    })
    const json = await response.json()
    console.log('Response:', json)
  } catch (error) {
    console.error('Failed to fetch:', error)
  }
})
```

### Signed Fetch
```typescript
import { executeTask } from '@dcl/sdk/ecs'
import { signedFetch } from '@dcl/sdk/network'

// Basic signed GET request
executeTask(async () => {
  try {
    const response = await signedFetch('https://api.example.com/data')
    const json = await response.json()
    console.log('Response:', json)
  } catch (error) {
    console.error('Failed to fetch:', error)
  }
})

// Signed POST request with headers and body
executeTask(async () => {
  try {
    const response = await signedFetch('https://api.example.com/data', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        key: 'value'
      })
    })
    const json = await response.json()
    console.log('Response:', json)
  } catch (error) {
    console.error('Failed to fetch:', error)
  }
})
```

### WebSocket Connections
```typescript
import { executeTask } from '@dcl/sdk/ecs'

// Basic WebSocket connection
executeTask(async () => {
  const ws = new WebSocket('wss://example.com/ws')
  
  ws.onopen = () => {
    console.log('Connected to WebSocket')
    ws.send('Hello Server!')
  }
  
  ws.onmessage = (event) => {
    console.log('Received:', event.data)
  }
  
  ws.onerror = (error) => {
    console.error('WebSocket error:', error)
  }
  
  ws.onclose = () => {
    console.log('Disconnected from WebSocket')
  }
})

// WebSocket with reconnection logic
executeTask(async () => {
  let ws: WebSocket | null = null
  let reconnectAttempts = 0
  const maxReconnectAttempts = 5
  
  function connect() {
    ws = new WebSocket('wss://example.com/ws')
    
    ws.onopen = () => {
      console.log('Connected to WebSocket')
      reconnectAttempts = 0
      ws?.send('Hello Server!')
    }
    
    ws.onmessage = (event) => {
      console.log('Received:', event.data)
    }
    
    ws.onerror = (error) => {
      console.error('WebSocket error:', error)
    }
    
    ws.onclose = () => {
      console.log('Disconnected from WebSocket')
      if (reconnectAttempts < maxReconnectAttempts) {
        reconnectAttempts++
        setTimeout(connect, 1000 * reconnectAttempts)
      }
    }
  }
  
  connect()
})
```

### Entity Synchronization
```typescript
import { syncEntity } from '@dcl/sdk/network'
import { engine, Transform, MeshRenderer } from '@dcl/sdk/ecs'
import { Vector3 } from '@dcl/sdk/math'

// Create a synced entity with a specific entityEnumId
const syncedEntity = engine.addEntity()
Transform.create(syncedEntity, { 
  position: Vector3.create(8, 1, 8),
  scale: Vector3.create(1, 1, 1)
})
MeshRenderer.setBox(syncedEntity)

// Sync the entity with other players
syncEntity(syncedEntity, [Transform.componentId], 1) // entityEnumId: 1

// Create a synced entity with multiple components
const complexEntity = engine.addEntity()
Transform.create(complexEntity, { 
  position: Vector3.create(5, 1, 5)
})
MeshRenderer.setBox(complexEntity)

// Sync multiple components
syncEntity(complexEntity, [
  Transform.componentId,
  MeshRenderer.componentId
], 2) // entityEnumId: 2

// Create a synced entity that responds to player interaction
const interactiveEntity = engine.addEntity()
Transform.create(interactiveEntity, { 
  position: Vector3.create(3, 1, 3)
})
MeshRenderer.setBox(interactiveEntity)

// Sync the entity and handle updates
syncEntity(interactiveEntity, [Transform.componentId], 3) // entityEnumId: 3

// Update synced entity position
Transform.getMutable(interactiveEntity).position = Vector3.create(4, 1, 4)
```

### Message Bus
```typescript
import { MessageBus } from '@dcl/sdk/message-bus'

// Create a message bus instance
const messageBus = new MessageBus()

// Define message types
type SpawnMessage = {
  position: { x: number; y: number; z: number }
  entityEnumId: number
}

type UpdateMessage = {
  entityId: number
  position: { x: number; y: number; z: number }
}

// Send a spawn message
messageBus.emit('spawn', {
  position: { x: 8, y: 1, z: 8 },
  entityEnumId: 1
} as SpawnMessage)

// Listen for spawn messages
messageBus.on('spawn', (message: SpawnMessage) => {
  const entity = engine.addEntity()
  Transform.create(entity, {
    position: Vector3.create(
      message.position.x,
      message.position.y,
      message.position.z
    )
  })
  MeshRenderer.setBox(entity)
  
  // Sync the newly created entity
  syncEntity(entity, [Transform.componentId], message.entityEnumId)
})

// Send an update message
messageBus.emit('update', {
  entityId: 1,
  position: { x: 10, y: 1, z: 10 }
} as UpdateMessage)

// Listen for update messages
messageBus.on('update', (message: UpdateMessage) => {
  // Find the entity by its synced ID
  const entity = engine.getEntityById(message.entityId)
  if (entity) {
    Transform.getMutable(entity).position = Vector3.create(
      message.position.x,
      message.position.y,
      message.position.z
    )
  }
})

// Example of a complete multiplayer interaction
function handlePlayerInteraction(entity: Entity) {
  // When a player interacts with an entity
  messageBus.emit('interaction', {
    entityId: entity,
    action: 'click',
    timestamp: Date.now()
  })
}

// Listen for player interactions
messageBus.on('interaction', (message) => {
  console.log(`Entity ${message.entityId} was ${message.action}ed at ${message.timestamp}`)
  // Handle the interaction for all players
})
```

## Blockchain Operations

### Get Player's Ethereum Account
```typescript
import { getPlayer } from '@dcl/sdk/src/players'

export function main() {
  let userData = getPlayer()
  if (!userData.isGuest) {
    console.log(userData.userId)
  } else {
    log('Player is not connected with Web3')
  }
}
```

### Check Gas Price
```typescript
import { RequestManager } from 'eth-connect'
import { createEthereumProvider } from '@dcl/sdk/ethereum-provider'

executeTask(async function () {
  // Create an instance of the web3 provider to interface with Metamask
  const provider = createEthereumProvider()
  // Create the object that will handle the sending and receiving of RPC messages
  const requestManager = new RequestManager(provider)
  // Check the current gas price on the Ethereum network
  const gasPrice = await requestManager.eth_gasPrice()
  // log response
  console.log({ gasPrice })
})
```

### Import Contract ABI
```typescript
// Example of one function in the MANA ABI
{
  anonymous: false,
  inputs: [
    {
      indexed: true,
      name: 'burner',
      type: 'address'
    },
    {
      indexed: false,
      name: 'value',
      type: 'uint256'
    }
  ],
  name: 'Burn',
  type: 'event'
}

// Import the ABI
import { abi } from '../contracts/mana'
```

### Instance a Contract
```typescript
import { RequestManager, ContractFactory } from 'eth-connect'
import { createEthereumProvider } from '@dcl/sdk/ethereum-provider'
import { abi } from '../contracts/mana'

executeTask(async () => {
  // Create an instance of the web3 provider to interface with Metamask
  const provider = createEthereumProvider()
  // Create the object that will handle the sending and receiving of RPC messages
  const requestManager = new RequestManager(provider)
  // Create a factory object based on the abi
  const factory = new ContractFactory(requestManager, abi)
  // Use the factory object to instance a `contract` object, referencing a specific contract
  const contract = (await factory.at(
    '0x2a8fd99c19271f4f04b1b7b9c4f7cf264b626edb'
  )) as any
})
```

### Call Contract Methods
```typescript
import { getPlayer } from '@dcl/sdk/src/players'
import { createEthereumProvider } from '@dcl/sdk/ethereum-provider'
import { RequestManager, ContractFactory } from 'eth-connect'
import { abi } from '../contracts/mana'

executeTask(async () => {
  try {
    // Setup steps explained in the section above
    const provider = createEthereumProvider()
    const requestManager = new RequestManager(provider)
    const factory = new ContractFactory(requestManager, abi)
    const contract = (await factory.at(
      '0x2a8fd99c19271f4f04b1b7b9c4f7cf264b626edb'
    )) as any
    let userData = getPlayer()
    if (userData.isGuest) {
      return
    }

    // Perform a function from the contract
    const res = await contract.setBalance(
      '0xaFA48Fad27C7cAB28dC6E970E4BFda7F7c8D60Fb',
      100,
      {
        from: userData.userId,
      }
    )
    // Log response
    console.log(res)
  } catch (error: any) {
    console.log(error.toString())
  }
})
```

### Send Custom RPC Messages
```typescript
import { sendAsync } from '~system/EthereumController'

// send a message
await sendAsync({
  id: 1,
  method: 'myMethod',
  jsonParams: '{ myParam: myValue }',
})
```