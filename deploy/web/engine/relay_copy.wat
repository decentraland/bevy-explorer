;; The only code in a sandbox worker that can reach engine memory. Imported by the relay's engine
;; half alone (crates/dcl_wasm/src/inner/relay.rs); never by the scene runtime or reachable from
;; scene js. The engine half passes engine addresses of its own buffers; the scene-memory range
;; comes from the scene runtime, so it is checked here, and an out-of-range copy returns 0 rather
;; than trapping in the engine half.
;; Build: npx -p wabt wat2wasm --enable-threads --enable-multi-memory relay_copy.wat (then regenerate relay_copy.js).
(module
  (import "relay" "engine" (memory $engine 1 65536 shared))
  (import "relay" "scene" (memory $scene 1 65536 shared))
  ;; 1 if [addr, addr + len) lies inside the scene memory
  (func $in_scene (param $addr i32) (param $len i32) (result i32)
    (i64.le_u
      (i64.add (i64.extend_i32_u (local.get $addr)) (i64.extend_i32_u (local.get $len)))
      (i64.mul (i64.extend_i32_u (memory.size $scene)) (i64.const 65536))))
  (func (export "copy_to_b") (param $src i32) (param $dst i32) (param $len i32) (result i32)
    (if (i32.eqz (call $in_scene (local.get $dst) (local.get $len)))
      (then (return (i32.const 0))))
    (memory.copy $scene $engine (local.get $dst) (local.get $src) (local.get $len))
    (i32.const 1))
  (func (export "copy_from_b") (param $dst i32) (param $src i32) (param $len i32) (result i32)
    (if (i32.eqz (call $in_scene (local.get $src) (local.get $len)))
      (then (return (i32.const 0))))
    (memory.copy $engine $scene (local.get $dst) (local.get $src) (local.get $len))
    (i32.const 1)))
