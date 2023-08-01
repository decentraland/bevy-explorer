// this code is executed as the runtime is created, all scopes get these definitions

// required for async ops (engine.sendMessage is declared as async)
// Deno.core.initializeAsyncOps();

// load a cjs/node-style module
// TODO: consider using deno.land/std/node's `createRequire` directly.
// Deno's node polyfill doesn't work without the full deno runtime, and i
// note that decentraland examples use ESM syntax which deno_core does support,
// so i haven't gone very deep into making full support work.
// this is a very simplified version of the deno_std/node `createRequire` implementation.
function require(moduleName) {
    // dynamically load the module source
    var source = Deno.core.ops.op_require(moduleName);

    // create a wrapper for the imported script
    source = source.replace(/^#!.*?\n/, "");
    const head = "(function (exports, require, module, __filename, __dirname) { (function (exports, require, module, __filename, __dirname) {";
    const foot = "\n}).call(this, exports, require, module, __filename, __dirname); })";
    source = `${head}${source}${foot}`;
    const [wrapped, err] = Deno.core.evalContext(source, "file://${moduleName}");
    if (err) {
        throw err.thrown;
    }

    // create minimal context for the execution
    var module = {
        exports: {}
    };
    // call the script
    // note: `require` function base path would need to be updated for proper support
    wrapped.call(
        module.exports,             // this
        module.exports,             // exports
        require,                    // require
        module,                     // module
        moduleName.substring(1),    // __filename
        moduleName.substring(0,1)   // __dirname
    );

    return module.exports;
}

// minimal console
const console = { 
    log: function(text) { Deno.core.ops.op_log("" + text) },
    error: function(text) { Deno.core.ops.op_error("" + text) },
}

// timeout handler
globalThis.setImmediate = (fn) => Promise.resolve().then(fn)