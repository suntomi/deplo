#!/bin/sh

#---------------- Argument ----------------
platform=${1}
config_file=${2}
exe_path=${3}
upload_only=${4}
bundle_id=${5}
upload_track=${6}
android_upload_key=${7}
ios_upload_account=${8}
ios_upload_account_pass=${9}

echo "---------------- Distribute MobileAppStore Start ----------------"
echo "[XcodeBuild]       : "$platform
echo "[XcodeProjectPath] : "$config_file
echo "[XcodeArchivePath] : "$exe_path
echo "[SchemeName]       : "$upload_only
echo "[ExportDir]        : "$bundle_id
echo "[OutputPath]       : "$upload_track
echo "[Timestamp]        : "$android_upload_key
echo "[OptionListPath]   : "$ios_upload_account
echo "[Configuration]    : "$ios_upload_account_pass


#---------------- Directory Setting ----------------
script_dir=$(cd $(dirname $0); pwd)

upload_opts=
if [ ! -z "$upload_only" ]; then
    upload_opts=--skip_waiting_for_build_processing
fi
if [ ! -z "$android_upload_key" ]; then
    android_upload_key_path="/tmp/$(echo $android_upload_key | sha1sum).json"
    echo $android_upload_key > $android_upload_key_path
fi

case $platform in 
"iOS" ) FASTLANE_PASSWORD=${ios_upload_account_pass} \
		DELIVER_ITMSTRANSPORTER_ADDITIONAL_UPLOAD_PARAMETERS="-t DAV" \
	    fastlane pilot upload --ipa $exe_path \
		--username ${ios_upload_account} \
		--reject_build_waiting_for_review $upload_opts ;;
"Android" ) fastlane supply --track ${upload_track} --apk $exe_path \
		--package_name ${bundle_id} --json_key ${android_upload_key_path} ;;
*) echo "unsupported $platform" && exit 1 ;;
esac

result=$?

echo "---------------- Distribute MobileAppStore End ($result) ----------------"
exit $result
