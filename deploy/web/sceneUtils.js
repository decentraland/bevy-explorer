// Scene utility functions for the browser console.
// Requires `engine_console_command` to be available on `window`.

async function findEntitiesByDistance(componentName) {
  const snapshot = await engine_console_command("/crdt_snapshot").then(JSON.parse);

  // Use the player entity (id=1) from the snapshot for scene-local position
  const playerPos = snapshot["1"]?.Transform
    ? getGlobalPosition("1", snapshot)
    : { x: 0, y: 0, z: 0 };

  const results = [];
  for (const [eid, components] of Object.entries(snapshot)) {
    if (componentName && !components[componentName]) continue;
    if (eid === "1" || eid === "0" || eid === "2") continue; // skip reserved entities
    const pos = getGlobalPosition(eid, snapshot);
    const dx = pos.x - playerPos.x, dy = pos.y - playerPos.y, dz = pos.z - playerPos.z;
    results.push({
      entity: eid,
      position: pos,
      dist: Math.sqrt(dx * dx + dy * dy + dz * dz),
      components,
    });
  }
  return results.sort((a, b) => a.dist - b.dist);
}

function getGlobalPosition(entityId, snapshot) {
  const chain = [];
  let current = entityId;
  while (current && current !== "0") {
    const t = snapshot[current]?.Transform;
    if (!t) break;
    chain.push(t);
    current = t.parent != null ? String(t.parent) : "0";
  }

  let gx = 0, gy = 0, gz = 0;
  let rot = { x: 0, y: 0, z: 0, w: 1 };
  let sx = 1, sy = 1, sz = 1;
  for (let i = chain.length - 1; i >= 0; i--) {
    const { position: p, rotation: r, scale: s } = chain[i];
    const scaled = { x: p.x * sx, y: p.y * sy, z: p.z * sz };
    const v = rotateByQuat(scaled, rot);
    gx += v.x; gy += v.y; gz += v.z;
    rot = multiplyQuat(rot, r);
    sx *= s.x; sy *= s.y; sz *= s.z;
  }
  return { x: gx, y: gy, z: gz };
}

function rotateByQuat(v, q) {
  const ix = q.w * v.x + q.y * v.z - q.z * v.y;
  const iy = q.w * v.y + q.z * v.x - q.x * v.z;
  const iz = q.w * v.z + q.x * v.y - q.y * v.x;
  const iw = -q.x * v.x - q.y * v.y - q.z * v.z;
  return {
    x: ix * q.w + iw * -q.x + iy * -q.z - iz * -q.y,
    y: iy * q.w + iw * -q.y + iz * -q.x - ix * -q.z,
    z: iz * q.w + iw * -q.z + ix * -q.y - iy * -q.x,
  };
}

function multiplyQuat(a, b) {
  return {
    x: a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
    y: a.w * b.y + a.y * b.w + a.z * b.x - a.x * b.z,
    z: a.w * b.z + a.z * b.w + a.x * b.y - a.y * b.x,
    w: a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
  };
}

function printEntitiesByDistance(componentName) {
  return findEntitiesByDistance(componentName).then(results =>
    console.table(
      results.map(r => ({
        entity: r.entity,
        x: r.position.x.toFixed(2),
        y: r.position.y.toFixed(2),
        z: r.position.z.toFixed(2),
        dist: r.dist.toFixed(2),
      }))
    )
  );
}
