#/bin/bash

set -e
echo "Building macOS app"

SOURCE_ICON_IMAGE="../linux/decentra-bevy.png"
APP_NAME="DecentralandBevyExplorer"

echo "Setting up the certificates"

TEMP_PASSWORD=$(openssl rand -base64 32)
security create-keychain -p "$TEMP_PASSWORD" build.keychain
security default-keychain -s build.keychain

echo -n $MACOS_CSC_CONTENT | base64 --decode -o file.p12
security unlock-keychain -p "$TEMP_PASSWORD" build.keychain
security import file.p12 -P "$MACOS_CSC_KEY_PASSWORD" -k build.keychain -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$TEMP_PASSWORD" build.keychain

echo "Create keychain profile"
xcrun notarytool store-credentials "notary-profile" --apple-id "$MACOS_NOTARIZATION_APPLE_ID" --team-id "$MACOS_NOTARIZATION_TEAM_ID" --password "$MACOS_NOTARIZATION_PWD"

echo "Creating AppIcon.icns"
mkdir -p AppIcon.iconset
sips -z 16 16     "${SOURCE_ICON_IMAGE}" --out AppIcon.iconset/icon_16x16.png
sips -z 32 32     "${SOURCE_ICON_IMAGE}" --out AppIcon.iconset/icon_16x16@2x.png
sips -z 32 32     "${SOURCE_ICON_IMAGE}" --out AppIcon.iconset/icon_32x32.png
sips -z 64 64     "${SOURCE_ICON_IMAGE}" --out AppIcon.iconset/icon_32x32@2x.png
sips -z 128 128   "${SOURCE_ICON_IMAGE}" --out AppIcon.iconset/icon_128x128.png
sips -z 256 256   "${SOURCE_ICON_IMAGE}" --out AppIcon.iconset/icon_128x128@2x.png
sips -z 256 256   "${SOURCE_ICON_IMAGE}" --out AppIcon.iconset/icon_256x256.png
sips -z 512 512   "${SOURCE_ICON_IMAGE}" --out AppIcon.iconset/icon_256x256@2x.png
sips -z 512 512   "${SOURCE_ICON_IMAGE}" --out AppIcon.iconset/icon_512x512.png
cp "${SOURCE_ICON_IMAGE}" AppIcon.iconset/icon_512x512@2x.png
iconutil -c icns AppIcon.iconset
rm -rf AppIcon.iconset

echo "Remove if exists"
rm -rf "${APP_NAME}.app"

echo "Create app folders"
mkdir -p "${APP_NAME}.app/Contents/MacOS"
mkdir -p "${APP_NAME}.app/Contents/Resources"

echo "Copy info and icons"
cp Info.plist "${APP_NAME}.app/Contents/Info.plist"
mv AppIcon.icns "${APP_NAME}.app/Contents/Resources/AppIcon.icns"

echo "Copy assets and binary"
cp -a ../../assets "${APP_NAME}.app/Contents/MacOS/"
cp -a ../../libs "${APP_NAME}.app/Contents/"
cp -a ../../decentra-bevy "${APP_NAME}.app/Contents/MacOS/${APP_NAME}"
cp -a ../../LICENSE "${APP_NAME}.app/Contents/MacOS/"


for LIBRARY in $APP_NAME.app/Contents/libs/*.dylib; do
    echo "Signing lib $LIBRARY"
    codesign --remove-signature "$LIBRARY"
    codesign --force --verify --verbose --sign "Developer ID Application: $MACOS_DEVELOPER_ID" "$LIBRARY"
done

echo "Sign the main binary"
codesign --entitlements entitlements.plist --deep --force --verify --options runtime --verbose --sign "Developer ID Application: $MACOS_DEVELOPER_ID" "$APP_NAME.app"

echo "Checking is signed"
codesign --verify --verbose "$APP_NAME.app"
codesign --display --entitlements :- $APP_NAME.app

echo "Notarize app"
ditto -c -k --sequesterRsrc --keepParent "$APP_NAME.app" "$APP_NAME.zip"
xcrun notarytool submit "$APP_NAME.zip" --keychain-profile "notary-profile" --wait

echo "Attach staple"
xcrun stapler staple $APP_NAME.app

echo "Validate staple"
xcrun stapler validate $APP_NAME.app

# TODO: need the developer id installer certificate

# echo "Create pkg"
# pkgbuild --root $APP_NAME.app \
#          --identifier "org.decentraland.bevyexplorer" \
#          --version "0.1.0" \
#          --install-location "/Applications" \
#          --scripts Scripts \
#          "$APP_NAME-unsigned.pkg"

# productsign --sign "Developer ID Installer: $MACOS_DEVELOPER_ID" \
#   "$APP_NAME-unsigned.pkg" "$APP_NAME.pkg"
  
# echo "Notarize pkg"
# xcrun notarytool submit "$APP_NAME.pkg" --keychain-profile "notary-profile" --wait

# echo "Attach staple pkg "
# xcrun stapler staple $APP_NAME.pkg

# echo "Validate staple pkg "
# xcrun stapler validate $APP_NAME.pkg
