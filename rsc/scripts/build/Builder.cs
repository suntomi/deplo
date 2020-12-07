using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;
using UnityEditor;
using UnityEngine;

namespace Deplo {
    public class UnityBuilder
    {
        // ----------------------------------------------------------------
        // Method
        // ----------------------------------------------------------------
        [MenuItem( "Tools/Deplo/Build/Debug/Android" )]
        public static void Build_Android()
        {
            EditorUserBuildSettings.SwitchActiveBuildTarget( BuildTargetGroup.Android, BuildTarget.Android );
            BuildApplication( EditorUserBuildSettings.activeBuildTarget );
        }

        [MenuItem( "Tools/Deplo/Build/Debug/iOS" )]
        public static void Build_iOS()
        {
            EditorUserBuildSettings.SwitchActiveBuildTarget( BuildTargetGroup.iOS, BuildTarget.iOS );
            BuildApplication( EditorUserBuildSettings.activeBuildTarget );
        }

        [MenuItem( "Tools/Deplo/Build/Release/Android" )]
        public static void Build_Relase_Android()
        {
            EditorUserBuildSettings.SwitchActiveBuildTarget( BuildTargetGroup.Android, BuildTarget.Android );
            BuildApplication( EditorUserBuildSettings.activeBuildTarget );
        }

        [MenuItem( "Tools/Deplo/Build/Release/iOS" )]
        public static void Build_Relase_iOS()
        {
            EditorUserBuildSettings.SwitchActiveBuildTarget( BuildTargetGroup.iOS, BuildTarget.iOS );
            BuildApplication( EditorUserBuildSettings.activeBuildTarget );
        }

        [MenuItem( "Tools/Deplo/Build/AndroidSdkInfo" )]
        public static void ShowAndroidSdkInfoFromMenu() {
            ShowAndroidSdkInfo();
        }

        public static void ShowAndroidSdkInfo(string tag = null)
        {
            if (tag != null) {
                UnityEngine.Debug.Log(tag);
            }
            UnityEngine.Debug.Log("AndroidNdkRootR19:" + EditorPrefs.GetString("AndroidNdkRootR19"));
            UnityEngine.Debug.Log("AndroidNdkRootR16b:"+ EditorPrefs.GetString("AndroidNdkRootR16b"));
            UnityEngine.Debug.Log("AndroidNdkRoot:" + EditorPrefs.GetString("AndroidNdkRoot"));
            UnityEngine.Debug.Log("AndroidSdkRoot:" + EditorPrefs.GetString("AndroidSdkRoot"));
        }

        private static void SetAndroidSdkPathForCI()
        {
            var ciUnityPath = System.Environment.GetEnvironmentVariable("DEPLO_UNITY_PATH");
            if (ciUnityPath != null) {
                var idx = ciUnityPath.IndexOf("Unity.app");
                if (idx >= 0) {
                    var unityRoot = ciUnityPath.Substring(0, idx);
                    var ndkRoot = System.IO.Path.Combine(unityRoot, "PlaybackEngines", "AndroidPlayer", "NDK");
                    var sdkRoot = System.IO.Path.Combine(unityRoot, "PlaybackEngines", "AndroidPlayer", "SDK");
                    ShowAndroidSdkInfo("before SetAndroidSdkPathForCI");
                    EditorPrefs.SetString("AndroidNdkRootR19", ndkRoot);
                    EditorPrefs.SetString("AndroidNdkRootR16b", ndkRoot);
                    EditorPrefs.SetString("AndroidNdkRoot", ndkRoot);
                    EditorPrefs.SetString("AndroidSdkRoot", sdkRoot);
                    ShowAndroidSdkInfo("after SetAndroidSdkPathForCI");
                } else {
                    UnityEngine.Debug.Log("SetAndroidSdkPathForCI: invalid unity editor path:" + ciUnityPath);
                }
            } else {
                UnityEngine.Debug.Log("SetAndroidSdkPathForCI: not CI");			
            }
        }

        private static string GetEnv( string key ) 
        {
            return System.Environment.GetEnvironmentVariable( key );
        }

        // アプリケーションビルド
        private static void BuildApplication( BuildTarget buildTarget )
        {
            try
            {
                Debug.Log( "Build Start --------------------------------" );

                // ビルドセッティングプッシュ
                PushBuildSetting( buildTarget );

                // ビルド実行
                RunBuild( buildTarget );
            }
            catch( System.Exception e )
            {
                throw new System.Exception( "Unity Build Failure. " + e );
            }
            finally
            {
                // アプリケーション情報ファイルクリア
    // 			ClearAppInfoFile();

                // ビルドセッティングポップ
                PopBuildSetting( buildTarget );

                Debug.Log( "Build End --------------------------------" );
            }
        }

        // ビルド実行
        private static void RunBuild( BuildTarget buildTarget )
        {
            try
            {
                // ビルドセッティング設定
                SetBuildSetting( buildTarget );

                // ビルドオプション設定
                var buildOptions = GetBuildOptions();

                // エクスポートパス作成
                var exportDirectory = GetEnv("DEPLO_UNITY_BUILD_EXPORT_PATH");
                Directory.CreateDirectory( exportDirectory );

                // エクスポートパス設定
                var buildName = GetBuildName( 
                    buildTarget, GetEnv("DEPLO_UNITY_APP_NAME")
                );
                var exportPath = exportDirectory + "/" + buildName;

                var buildPlayerOptions = new BuildPlayerOptions();
                buildPlayerOptions.scenes = GetScenes();
                buildPlayerOptions.locationPathName = Path.GetFullPath( exportPath );
                buildPlayerOptions.target = buildTarget;
                buildPlayerOptions.options = buildOptions;
                var buildResult = BuildPipeline.BuildPlayer( buildPlayerOptions );

                if( buildResult.summary.result != UnityEditor.Build.Reporting.BuildResult.Succeeded )
                {
                    var msg = string.Join(",", 
                        buildResult.steps[buildResult.steps.Length - 1].messages.Select((m) => m.content).ToArray()
                    );
                    Debug.Log( "BuildError[" + msg + "]" );

                    throw new System.Exception( "Unity Build Error:" + msg );
                }
                else
                {
                    Debug.Log( "Unity Build Success." );
                }
            }
            catch( System.Exception e )
            {
                throw new System.Exception( "Unity Build Error. " + e );
            }
        }

        // ビルドセッティング設定
        private static void SetBuildSetting( BuildTarget buildTarget )
        {
            Debug.Log( "==== SetBuildSetting ====" );

            PlayerSettings.companyName = GetEnv("DEPLO_UNITY_COMPANY_NAME");
            PlayerSettings.productName = GetEnv("DEPLO_UNITY_APP_NAME");
            PlayerSettings.SetApplicationIdentifier( GetBuildTargetGroup( buildTarget ), GetEnv("DEPLO_UNITY_APP_ID") );

            switch( buildTarget )
            {
            case BuildTarget.iOS:
                PlayerSettings.bundleVersion = GetEnv("DEPLO_UNITY_APP_VERSION");
                PlayerSettings.iOS.buildNumber = GetEnv("DEPLO_UNITY_INTERNAL_BUILD_NUMBER");

                PlayerSettings.iOS.targetOSVersionString = BuildConfig.iOSTargetOSVersion;

                PlayerSettings.iOS.sdkVersion = iOSSdkVersion.DeviceSDK;

                PlayerSettings.iOS.applicationDisplayName = GetEnv("DEPLO_UNITY_APP_VERSION");

                if( string.IsNullOrEmpty(GetEnv("DEPLO_UNITY_IOS_AUTOMATIC_SIGN")) )
                {
                    PlayerSettings.iOS.appleEnableAutomaticSigning = false;
                    PlayerSettings.iOS.iOSManualProvisioningProfileID = GetEnv("DEPLO_UNITY_IOS_PROVISION_ID");
                }
                else
                {
                    PlayerSettings.iOS.appleEnableAutomaticSigning = true;
                    PlayerSettings.iOS.appleDeveloperTeamID = GetEnv("DEPLO_UNITY_IOS_TEAM_ID");
                }
                break;
            case BuildTarget.Android:
                PlayerSettings.bundleVersion = GetEnv("DEPLO_UNITY_APP_VERSION");
                PlayerSettings.Android.bundleVersionCode = int.Parse( GetEnv("DEPLO_UNITY_INTERNAL_BUILD_NUMBER"); );
                if (PlayerSettings.Android.bundleVersionCode <= 0) {
                    UnityEngine.Debug.Log("tweak PlayerSettings.Android.bundleVersionCode to be positive: original:" + 
                        PlayerSettings.Android.bundleVersionCode.ToString()
                    );
                    PlayerSettings.Android.bundleVersionCode = 1;
                }

                PlayerSettings.Android.minSdkVersion = GetAndroidSdkVersion( BuildConfig.androidMinSdkVersion );
                PlayerSettings.Android.targetSdkVersion = GetAndroidSdkVersion( BuildConfig.androidTargetSdkVersion );

                PlayerSettings.Android.targetArchitectures = AndroidArchitecture.All;

                PlayerSettings.Android.keystoreName = GetEnv("DEPLO_UNITY_ANDROID_KEYSTORE_PATH");
                PlayerSettings.Android.keystorePass = GetEnv("DEPLO_UNITY_ANDROID_KEYSTORE_PASSWORD");
                PlayerSettings.Android.keyaliasName = GetEnv("DEPLO_UNITY_ANDROID_KEYALIAS_NAME");
                PlayerSettings.Android.keyaliasPass = GetEnv("DEPLO_UNITY_ANDROID_KEYALIAS_PASSWORD");

                PlayerSettings.Android.useAPKExpansionFiles = 
                    !string.IsNullOrEmpty(GetEnv("DEPLO_UNITY_ANDROID_USE_EXPANSION_FILE"));

                SetAndroidSdkPathForCI();
                break;
            case BuildTarget.StandaloneOSX:
                break;
            case BuildTarget.StandaloneLinux:
            case BuildTarget.StandaloneLinux64:
            case BuildTarget.StandaloneLinuxUniversal:
                break;
            case BuildTarget.StandaloneWindows:
            case BuildTarget.StandaloneWindows64:
                break;
            default:
                break;
            }

            // 定義設定
            PlayerSettings.SetScriptingDefineSymbolsForGroup( GetBuildTargetGroup( buildTarget ), GetEnv("DEPLO_UNITY_APP_VERSION") );
        }

        // ビルドオプション取得
        private static BuildOptions GetBuildOptions()
        {
            Debug.Log( "==== GetBuildOptions ====" );

            BuildOptions buildOptions = BuildOptions.None;

            buildOptions |= BuildOptions.SymlinkLibraries;

            if( GetEnv("DEPLO_UNITY_BUILD_PROFILE") == "Debug" )
            {
                buildOptions |= BuildOptions.Development;
            }

            return buildOptions;
        }

        // シーン取得
        private static string[] GetScenes()
        {
            Debug.Log( "==== GetScenes ====" );

            var query =
                from scene in EditorBuildSettings.scenes
                select scene.path
                ;

            return query.ToArray();
        }

        // ビルドセッティングプッシュ
        private static void PushBuildSetting( BuildTarget buildTarget )
        {
            Debug.Log( "==== PushBuildSetting ====" );

            m_TemporaryPlayerSettings = new Dictionary<string, object>();

            m_TemporaryPlayerSettings[ "ScriptingDefineSymbolsForGroup" ] = PlayerSettings.GetScriptingDefineSymbolsForGroup( GetBuildTargetGroup( buildTarget ) );

            m_TemporaryPlayerSettings[ "companyName" ] = PlayerSettings.companyName;
            m_TemporaryPlayerSettings[ "productName" ] = PlayerSettings.productName;
            m_TemporaryPlayerSettings[ "applicationIdentifier" ] = PlayerSettings.applicationIdentifier;
            m_TemporaryPlayerSettings[ "bundleVersion" ] = PlayerSettings.bundleVersion;

            m_TemporaryPlayerSettings[ "iOS_buildNumber" ] = PlayerSettings.iOS.buildNumber;
            m_TemporaryPlayerSettings[ "iOS_buildtargetOSVersionString" ] = PlayerSettings.iOS.targetOSVersionString;
            m_TemporaryPlayerSettings[ "iOS_sdkVersion" ] = PlayerSettings.iOS.sdkVersion;
            m_TemporaryPlayerSettings[ "iOS_applicationDisplayName" ] = PlayerSettings.iOS.applicationDisplayName;
            m_TemporaryPlayerSettings[ "iOS_appleEnableAutomaticSigning" ] = PlayerSettings.iOS.appleEnableAutomaticSigning;
            m_TemporaryPlayerSettings[ "iOS_iOSManualProvisioningProfileID" ] = PlayerSettings.iOS.iOSManualProvisioningProfileID;
            m_TemporaryPlayerSettings[ "iOS_appleDeveloperTeamID" ] = PlayerSettings.iOS.appleDeveloperTeamID;

            m_TemporaryPlayerSettings[ "Android_bundleVersionCode" ] = PlayerSettings.Android.bundleVersionCode;
            m_TemporaryPlayerSettings[ "Android_minSdkVersion" ] = PlayerSettings.Android.minSdkVersion;
            m_TemporaryPlayerSettings[ "Android_targetDevice" ] = PlayerSettings.Android.targetArchitectures;
            m_TemporaryPlayerSettings[ "Android_keystoreName" ] = PlayerSettings.Android.keystoreName;
            m_TemporaryPlayerSettings[ "Android_keystorePass" ] = PlayerSettings.Android.keystorePass;
            m_TemporaryPlayerSettings[ "Android_keyaliasName" ] = PlayerSettings.Android.keyaliasName;
            m_TemporaryPlayerSettings[ "Android_keyaliasPass" ] = PlayerSettings.Android.keyaliasPass;

            var ciUnityPath = GetEnv("DEPLO_UNITY_PATH");
            if (ciUnityPath != null) {
                m_TemporaryPlayerSettings[ "AndroidNdkRootR19" ] = EditorPrefs.GetString("AndroidNdkRootR19");
                m_TemporaryPlayerSettings[ "AndroidNdkRootR16b" ] = EditorPrefs.GetString("AndroidNdkRootR16b");
                m_TemporaryPlayerSettings[ "AndroidNdkRoot" ] = EditorPrefs.GetString("AndroidNdkRoot");
                m_TemporaryPlayerSettings[ "AndroidSdkRoot" ] = EditorPrefs.GetString("AndroidSdkRoot");
            }
        }

        // ビルドセッティングポップ
        private static void PopBuildSetting( BuildTarget buildTarget )
        {
            Debug.Log( "==== PopBuildSetting ====" );

            PlayerSettings.SetScriptingDefineSymbolsForGroup( GetBuildTargetGroup( buildTarget ), m_TemporaryPlayerSettings[ "ScriptingDefineSymbolsForGroup" ] as string );

            PlayerSettings.companyName = m_TemporaryPlayerSettings[ "companyName" ] as string;
            PlayerSettings.productName = m_TemporaryPlayerSettings[ "productName" ] as string;
            PlayerSettings.SetApplicationIdentifier( GetBuildTargetGroup( buildTarget ), m_TemporaryPlayerSettings[ "applicationIdentifier" ] as string );
            PlayerSettings.bundleVersion = m_TemporaryPlayerSettings[ "bundleVersion" ] as string;

            PlayerSettings.iOS.buildNumber = m_TemporaryPlayerSettings[ "iOS_buildNumber" ] as string;
            PlayerSettings.iOS.targetOSVersionString = m_TemporaryPlayerSettings[ "iOS_buildtargetOSVersionString" ] as string;
            PlayerSettings.iOS.sdkVersion = (iOSSdkVersion)m_TemporaryPlayerSettings[ "iOS_sdkVersion" ];
            PlayerSettings.iOS.applicationDisplayName = m_TemporaryPlayerSettings[ "iOS_applicationDisplayName" ] as string;
            PlayerSettings.iOS.appleEnableAutomaticSigning = (bool)m_TemporaryPlayerSettings[ "iOS_appleEnableAutomaticSigning" ];
            PlayerSettings.iOS.iOSManualProvisioningProfileID = m_TemporaryPlayerSettings[ "iOS_iOSManualProvisioningProfileID" ] as string;
            PlayerSettings.iOS.appleDeveloperTeamID = m_TemporaryPlayerSettings[ "iOS_appleDeveloperTeamID" ] as string;

            PlayerSettings.Android.bundleVersionCode = (int)m_TemporaryPlayerSettings[ "Android_bundleVersionCode" ];
            PlayerSettings.Android.minSdkVersion = (AndroidSdkVersions)m_TemporaryPlayerSettings[ "Android_minSdkVersion" ];
            PlayerSettings.Android.targetArchitectures = (AndroidArchitecture)m_TemporaryPlayerSettings[ "Android_targetDevice" ];
            PlayerSettings.Android.keystoreName = m_TemporaryPlayerSettings[ "Android_keystoreName" ] as string;
            PlayerSettings.Android.keystorePass = m_TemporaryPlayerSettings[ "Android_keystorePass" ] as string;
            PlayerSettings.Android.keyaliasName = m_TemporaryPlayerSettings[ "Android_keyaliasName" ] as string;
            PlayerSettings.Android.keyaliasPass = m_TemporaryPlayerSettings[ "Android_keyaliasPass" ] as string;

            var ciUnityPath = System.Environment.GetEnvironmentVariable("TK2L_UNITY_PATH");
            if (ciUnityPath != null) {
                EditorPrefs.SetString("AndroidNdkRootR19", (string)m_TemporaryPlayerSettings[ "AndroidNdkRootR19" ]);
                EditorPrefs.SetString("AndroidNdkRootR16b", (string)m_TemporaryPlayerSettings[ "AndroidNdkRootR16b" ]);
                EditorPrefs.SetString("AndroidNdkRoot", (string)m_TemporaryPlayerSettings[ "AndroidNdkRoot" ]);
                EditorPrefs.SetString("AndroidSdkRoot", (string)m_TemporaryPlayerSettings[ "AndroidSdkRoot" ]);
            }

            m_TemporaryPlayerSettings.Clear();
            m_TemporaryPlayerSettings = null;
        }

        // ビルド名取得
        private static string GetBuildName( BuildTarget buildTarget, string appName )
        {
            Debug.Log( "==== GetBuildName ====" );

            switch( buildTarget )
            {
            case BuildTarget.iOS:
                return appName;
            case BuildTarget.Android:
                return appName + ".apk";
            case BuildTarget.StandaloneOSX:
                return appName + ".app";
            case BuildTarget.StandaloneLinux:
            case BuildTarget.StandaloneLinux64:
            case BuildTarget.StandaloneLinuxUniversal:
                return appName;
            case BuildTarget.StandaloneWindows:
            case BuildTarget.StandaloneWindows64:
                return appName + ".exe";
            }

            return null;
        }

        // ビルドターゲットグループ取得
        private static BuildTargetGroup GetBuildTargetGroup( BuildTarget buildTarget )
        {
            Debug.Log( "==== BuildTargetGroup ====" );

            switch( buildTarget )
            {
            case BuildTarget.iOS:
                return BuildTargetGroup.iOS;
            case BuildTarget.Android:
                return BuildTargetGroup.Android;
            case BuildTarget.StandaloneOSX:
                return BuildTargetGroup.Standalone;
            case BuildTarget.StandaloneLinux:
            case BuildTarget.StandaloneLinux64:
            case BuildTarget.StandaloneLinuxUniversal:
                return BuildTargetGroup.Standalone;
            case BuildTarget.StandaloneWindows:
            case BuildTarget.StandaloneWindows64:
                return BuildTargetGroup.Standalone;
            }

            return BuildTargetGroup.Unknown;
        }

        // AndroidSDKバージョン取得
        private static AndroidSdkVersions GetAndroidSdkVersion( string sdkVersion )
        {
            switch( sdkVersion )
            {
                case "19":
                    return AndroidSdkVersions.AndroidApiLevel19;
                case "21":
                    return AndroidSdkVersions.AndroidApiLevel21;
                case "22":
                    return AndroidSdkVersions.AndroidApiLevel22;
                case "23":
                    return AndroidSdkVersions.AndroidApiLevel23;
                case "24":
                    return AndroidSdkVersions.AndroidApiLevel24;
                case "25":
                    return AndroidSdkVersions.AndroidApiLevel25;
                case "26":
                    return AndroidSdkVersions.AndroidApiLevel26;
                case "27":
                    return AndroidSdkVersions.AndroidApiLevel27;
                case "28":
                    return AndroidSdkVersions.AndroidApiLevel28;
                case "29":
                    // TODO AndroidApiLevel29 は Unity 2020以降らしいので、とりあえず強引に29対応にしておく
                    return (AndroidSdkVersions)29;
            }

            return AndroidSdkVersions.AndroidApiLevelAuto;
        }

        // ----------------------------------------------------------------
        // Field
        // ----------------------------------------------------------------
        private static Dictionary<string, object> m_TemporaryPlayerSettings = null;
    }
}