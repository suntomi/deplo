# Crashlyticsにdsymをアップロードする
if [ "$platform" = "iOS" ]; then
	if [ -f "/tmp/build/xcode_project_path" ]; then
		xcode_project_path=$(cat /tmp/build/xcode_project_path)
		echo "/tmp/build/xcode_project_path found. Start uploading syms to Crashlytics."
		find /tmp/build/iOS/syms -name "*.dSYM" | xargs -I \{\} "${xcode_project_path}/Pods/Fabric/upload-symbols" -gsp "${xcode_project_path}/GoogleService-Info.plist" -p ios \{\}
	else
		echo "/tmp/build/xcode_project_path not found. Skip uploading syms to Crashlytics."
	fi
fi

