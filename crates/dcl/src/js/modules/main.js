function require(moduleName) {
    var source = globalThis.js_require(moduleName);
    if (!source) {
        throw new Error("Module not found: " + moduleName)
    }

    // create a wrapper for the imported script
    source = source.replace(/^#!.*?\n/, "");
    const head = "(function (exports, require, module, __filename, __dirname) { (function (exports, require, module, __filename, __dirname) {";
    const foot = "\n}).call(this, exports, require, module, __filename, __dirname); })";
    source = `${head}${source}${foot}`;
    const wrapped = eval(source);

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
        moduleName.substring(0, 1)   // __dirname
    );

    return module.exports;
}

globalThis.require = require
globalThis.setImmediate = (fn) => Promise.resolve().then(fn)
globalThis.console = {
    log: function (text) { console_log("" + text) },
    error: function (text) { console_error("" + text) },
}


const scene = require('~scene.js');
globalThis.onStart = scene.onStart;
globalThis.onUpdate = scene.onUpdate;