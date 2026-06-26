# Documentation for `getBackgroundFromAtlas`

This document explains how to set up and use the `getBackgroundFromAtlas` function in your TypeScript project. This utility allows you to extract textures from a texture atlas and apply them to UI entities in your project.

---

## Setting Up Your Texture Atlas

To use `getBackgroundFromAtlas`, you must prepare and organize your texture atlas and its corresponding metadata as follows:

1. **Save the Texture Atlas Image**  
   Place your texture atlas image (in `.png` format) in the following directory:  
   `scene/assets/images/atlas/`  
   For example: scene/assets/images/atlas/atlas.png


2. **Modifying the Helper Function getUvs**
Ensure that the metadata file is included in the getUvs function. Add the exported JSON constant to the switch statement based on the atlas name. Example:

```typescript
export type AtlasIcon = { atlasName: string; spriteName: string }

export function getUvs(icon: AtlasIcon): number[] {
  let parsedJson: AtlasData | undefined;
  switch (icon.atlasName) {
    case 'atlas':
      parsedJson = atlasJson; // Include your atlas JSON
      break;
    // Add other atlas cases here
  }
  ...
}
```

3. **Using getBackgroundFromAtlas**
The getBackgroundFromAtlas function takes an Icon object as input and returns a UiBackgroundProps object with the appropriate texture and UV mapping. Example usage:

```typescript
export function getBackgroundFromAtlas(icon: AtlasIcon): UiBackgroundProps {
  const textureMode = 'stretch';
  const uvs = getUvs(icon);
  const texture = { src: `assets/images/atlas/${icon.atlasName}.png` };
  return {
    textureMode,
    uvs,
    texture
  };
}
```

4. **Applying the Texture to a UI Entity**
To use the texture in your UI, apply it to a <UiEntity> component using the getBackgroundFromAtlas function.

Example:

```typescript
<UiEntity
  uiTransform={{
    width: '70%',
    height: '70%',
    flexDirection: 'row',
    alignItems: 'center'
  }}
  uiBackground={getBackgroundFromAtlas({
    atlasName: 'atlas', // Name of your atlas
    spriteName: 'sprite' // Name of your sprite
  })}
/>
```
---
**Notes and Best Practices**

Atlas Metadata:

- The spriteName should match one of the keys in the frames object of your atlas metadata (e.g., "sprite.png").
Switch Statement Updates:

- Always add a case for each new atlas in the getUvs function to ensure proper mapping.
