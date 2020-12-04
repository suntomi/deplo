#!/bin/bash
project_namespace=${1}
signing_cert_p12=${2}
signing_development_cert_p12=${3}
provisioning_profile_source=${4}
provisioning_profile=${5}

echo "---------------- CreateKeyChain Start ----------------"
echo "[ProjectNamespace]          : "$project_namespace
echo "[SigningCertP12]            : "$signing_cert_p12
echo "[XcodeArchivePath]          : "$signing_development_cert_p12
echo "[ProvisioningProfileSource] : "$provisioning_profile_source
echo "[ProvisioningProfile]       : "$provisioning_profile

# generate specific keychain for the build and add cert for profile
# because on CI, underlying osx machine is not stable, no assurance that setup keychain is exist. 
# https://stackoverflow.com/questions/16550594/jenkins-xcode-build-works-codesign-fails
keychain_path="/tmp/${project_namespace}.keychain"
keychain_password="pass"
security delete-keychain $keychain_path
security create-keychain -p $keychain_password $keychain_path

# securty default-keychain returns path enclosing whitespace and double quotation. 
default_keychain_path=`security default-keychain | sed -e 's/^[[:space:]]*//' | tr -d \"`
# add temporary keychain to keychain search list
security list-keychains -s "$keychain_path"
# set temporary keychain as default
security default-keychain -s "$keychain_path"
# unlock temporary keychain with no timeout (set-keychain-settings)
security set-keychain-settings "$keychain_path"
security unlock-keychain -p "$keychain_password" "$keychain_path"
# import keychain and allow access to /usr/bin/codesign
security import $signing_cert_p12 -t agg -k $keychain_path -P $TK2L_IOS_SIGNING_P12_PASSWORD -T /usr/bin/codesign
security import $signing_development_cert_p12 -t agg -k $keychain_path -P $TK2L_IOS_SIGNING_P12_PASSWORD -T /usr/bin/codesign
# allow apple tools to use keychain without asking password
security set-key-partition-list -S apple-tool:,apple: -s -k $keychain_password $keychain_path

# copy provisioning profile with proper name
mkdir -p $HOME/Library/MobileDevice/Provisioning\ Profiles/
echo "COPY: cp $script_dir/../../$provisioning_profile_source $HOME/Library/MobileDevice/Provisioning\ Profiles/$provisioning_profile.mobileprovision"
cp $script_dir/../../$provisioning_profile_source $HOME/Library/MobileDevice/Provisioning\ Profiles/$provisioning_profile.mobileprovision

echo "CHAIN-LIST: $(security list-keychains)"
echo "CHAIN-DEFAULT: $(security default-keychain)"
security find-identity -v -p codesigning

echo "---------------- CreateKeyChain End (0)----------------"
