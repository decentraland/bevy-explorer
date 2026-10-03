// The gates' port set (e2e/README.md "Running gates side by side"): the page, its bridge scene,
// the project storage service and the Worlds server.

const port = (name: string, fallback: number): number => Number(process.env[name] ?? fallback)

export const GATE_PORTS = {
  page: port('GATE_PORT', 5230),
  bridge: port('GATE_BRIDGE_PORT', 8110),
  service: port('GATE_SERVICE_PORT', 8787),
  worlds: port('GATE_WORLDS_PORT', 8799)
}
