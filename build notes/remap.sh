#!/bin/bash
set -e

# --- Configuration ---
LIB_INPUT="libwebrtc.a"
LIB_OUTPUT="libwebrtc_prefixed.a"
PREFIX="webrtc_"

# The "Nuclear" list of prefixes (FFmpeg + BoringSSL/OpenSSL)
# We use a regex that handles optional leading underscores (_) for macOS support
PATTERN="(_)?(av|sws|swr|avio|avcodec|avformat|avfilter|avutil|avdevice|SSL|ssl|CRYPTO|crypto|BIO|bio|EVP|evp|RSA|rsa|X509|x509|PEM|pem|ERR|err|BN|bn|EC|ec|PKCS|pkcs)_"
# --- Tool Detection ---
OS="$(uname -s)"
if [ "$OS" = "Darwin" ]; then
    echo "[*] macOS detected."
    # Try to find llvm-objcopy via Homebrew if not in PATH
    if command -v llvm-objcopy &> /dev/null; then
        OBJCOPY="llvm-objcopy"
        NM="llvm-nm"
    elif [ -f "/opt/homebrew/opt/llvm/bin/llvm-objcopy" ]; then
        OBJCOPY="/opt/homebrew/opt/llvm/bin/llvm-objcopy"
        NM="/opt/homebrew/opt/llvm/bin/llvm-nm"
    else
        echo "Error: llvm-objcopy not found. Please run: brew install llvm"
        exit 1
    fi
else
    echo "[*] Linux detected."
    OBJCOPY="objcopy"
    NM="nm"
fi

echo "[*] Using tools: $NM and $OBJCOPY"

# --- Step 1: Generate Symbol Map ---
echo "[1/3] Scanning $LIB_INPUT for colliding symbols..."

echo "pattern: $PATTERN"

# 1. List symbols
# 2. Grep for our pattern (av_, SSL_, etc)
# 3. Extract the symbol name (last column)
# 4. Sort and Uniq
$NM --defined-only -g "$LIB_INPUT" \
    | grep -E " $PATTERN" \
    | awk '{print $NF}' \
    | sort | uniq > symbols_found.txt

COUNT=$(wc -l < symbols_found.txt)
if [ "$COUNT" -eq "0" ]; then
    echo "Error: No matching symbols found! Check your input library."
    exit 1
fi
echo "    Found $COUNT symbols to rename."

# --- Step 2: Create Map File ---
echo "[2/3] Generating redefinition map..."

# We create a map: "old_symbol new_symbol"
# Note: On Mac, old_symbol might be "_av_malloc". 
# We simply prepend the prefix, resulting in "webrtc__av_malloc". 
# This is ugly (double underscore) but perfectly safe and robust.
> symbols.map
while read -r sym; do
    echo "$sym $PREFIX$sym" >> symbols.map
done < symbols_found.txt

# --- Step 3: Rename ---
echo "[3/3] Renaming symbols..."

$OBJCOPY --redefine-syms=symbols.map "$LIB_INPUT" "$LIB_OUTPUT"

echo "---------------------------------------------------"
echo "Success! Created $LIB_OUTPUT"
echo "Use this library and include 'webrtc_renames.h' in your project."
