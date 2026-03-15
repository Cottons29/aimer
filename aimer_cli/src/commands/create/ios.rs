use std::fs;
use std::path::PathBuf;

pub fn create(dir: &PathBuf) {
    let project_name = dir.file_name().unwrap().to_str().unwrap();
    let project_name_lib = project_name.replace("-", "_");
    let ios_dir = dir.join("builds/ios");
    fs::create_dir_all(&ios_dir).unwrap();
    fs::create_dir_all(ios_dir.join(format!("{}.xcodeproj", project_name))).unwrap();
    
    fs::write(
        ios_dir.join(format!("{}.xcodeproj/project.pbxproj", project_name)),
        format!(
            r#"// !$*UTF8*$!
{{
	archiveVersion = 1;
	classes = {{
	}};
	objectVersion = 46;
	objects = {{

/* Begin PBXBuildFile section */
		C7A3B2D1F5E83D7B1E6F5092 /* main.swift in Sources */ = {{isa = PBXBuildFile; fileRef = B3F2A1C0E4B72C6A0D5E4F81 /* main.swift */; }};
		2471626C6643ED4FC7F810D3 /* Foundation.framework in Frameworks */ = {{isa = PBXBuildFile; fileRef = 0B72399071DA7A9D3A3629F8 /* Foundation.framework */; }};
/* End PBXBuildFile section */

/* Begin PBXFileReference section */
		B3F2A1C0E4B72C6A0D5E4F81 /* main.swift */ = {{isa = PBXFileReference; fileEncoding = 4; lastKnownFileType = sourcecode.swift; path = main.swift; sourceTree = "<group>"; }};
		0B72399071DA7A9D3A3629F8 /* Foundation.framework */ = {{isa = PBXFileReference; lastKnownFileType = wrapper.framework; name = Foundation.framework; path = Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS18.0.sdk/System/Library/Frameworks/Foundation.framework; sourceTree = DEVELOPER_DIR; }};
		992093C02504A3F306172469 /* {project_name}.app */ = {{isa = PBXFileReference; explicitFileType = wrapper.application; includeInIndex = 0; path = {project_name}.app; sourceTree = BUILT_PRODUCTS_DIR; }};
/* End PBXFileReference section */

/* Begin PBXFrameworksBuildPhase section */
		670D25E833A78BAF2A1855F7 /* Frameworks */ = {{
			isa = PBXFrameworksBuildPhase;
			buildActionMask = 2147483647;
			files = (
				2471626C6643ED4FC7F810D3 /* Foundation.framework in Frameworks */,
			);
			runOnlyForDeploymentPostprocessing = 0;
		}};
/* End PBXFrameworksBuildPhase section */

/* Begin PBXGroup section */
		076DC3D42E3B059254AD8B27 /* Frameworks */ = {{
			isa = PBXGroup;
			children = (
				EBAD59C9D065DFC23F7D3643 /* iOS */,
			);
			name = Frameworks;
			sourceTree = "<group>";
		}};
		605947B78B2A2F74560FF371 /* Products */ = {{
			isa = PBXGroup;
			children = (
				992093C02504A3F306172469 /* {project_name}.app */,
			);
			name = Products;
			sourceTree = "<group>";
		}};
		835F924D3E646A9F762B34B2 /* {project_name} */ = {{
			isa = PBXGroup;
			children = (
				B3F2A1C0E4B72C6A0D5E4F81 /* main.swift */,
			);
			path = {project_name};
			sourceTree = "<group>";
		}};
		A4F672D615B5CA5001D28519 = {{
			isa = PBXGroup;
			children = (
				835F924D3E646A9F762B34B2 /* {project_name} */,
				605947B78B2A2F74560FF371 /* Products */,
				076DC3D42E3B059254AD8B27 /* Frameworks */,
			);
			sourceTree = "<group>";
		}};
		EBAD59C9D065DFC23F7D3643 /* iOS */ = {{
			isa = PBXGroup;
			children = (
				0B72399071DA7A9D3A3629F8 /* Foundation.framework */,
			);
			name = iOS;
			sourceTree = "<group>";
		}};
/* End PBXGroup section */

/* Begin PBXNativeTarget section */
		7648BF25D679D16DF8E7E6F4 /* {project_name} */ = {{
			isa = PBXNativeTarget;
			buildConfigurationList = E78EF380B86DAD433C3C7F9E /* Build configuration list for PBXNativeTarget "{project_name}" */;
			buildPhases = (
				3A50768CE9D6E3D95F4987D4 /* Sources */,
				670D25E833A78BAF2A1855F7 /* Frameworks */,
				D2F62A4BCEF723F1016DEB73 /* Resources */,
			);
			buildRules = (
			);
			dependencies = (
			);
			name = {project_name};
			productName = {project_name};
			productReference = 992093C02504A3F306172469 /* {project_name}.app */;
			productType = "com.apple.product-type.application";
		}};
/* End PBXNativeTarget section */

/* Begin PBXProject section */
		F745F1A60BF1B4F8EBEABC66 /* Project object */ = {{
			isa = PBXProject;
			attributes = {{
				LastSwiftUpdateCheck = 1600;
				LastUpgradeCheck = 1600;
			}};
			buildConfigurationList = 2387A685D112612CDD6DD78D /* Build configuration list for PBXProject "{project_name}" */;
			compatibilityVersion = "Xcode 3.2";
			developmentRegion = en;
			hasScannedForEncodings = 0;
			knownRegions = (
				en,
				Base,
			);
			mainGroup = A4F672D615B5CA5001D28519;
			minimizedProjectReferenceProxies = 0;
			preferredProjectObjectVersion = 77;
			productRefGroup = 605947B78B2A2F74560FF371 /* Products */;
			projectDirPath = "";
			projectRoot = "";
			targets = (
				7648BF25D679D16DF8E7E6F4 /* {project_name} */,
			);
		}};
/* End PBXProject section */

/* Begin PBXResourcesBuildPhase section */
		D2F62A4BCEF723F1016DEB73 /* Resources */ = {{
			isa = PBXResourcesBuildPhase;
			buildActionMask = 2147483647;
			files = (
			);
			runOnlyForDeploymentPostprocessing = 0;
		}};
/* End PBXResourcesBuildPhase section */

/* Begin PBXSourcesBuildPhase section */
		3A50768CE9D6E3D95F4987D4 /* Sources */ = {{
			isa = PBXSourcesBuildPhase;
			buildActionMask = 2147483647;
			files = (
				C7A3B2D1F5E83D7B1E6F5092 /* main.swift in Sources */,
			);
			runOnlyForDeploymentPostprocessing = 0;
		}};
/* End PBXSourcesBuildPhase section */

/* Begin XCBuildConfiguration section */
		22844781F1EDECBF9F17C0BA /* Debug */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
				ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon;
				ASSETCATALOG_COMPILER_GLOBAL_ACCENT_COLOR_NAME = AccentColor;
				GENERATE_INFOPLIST_FILE = NO;
				INFOPLIST_FILE = "{project_name}/Info.plist";
				OTHER_LDFLAGS = (
					"-L$(SRCROOT)/../staticlib/ios",
					"-l{project_name_lib}",
					"-lc++",
					"-framework",
					UIKit,
					"-framework",
					Metal,
					"-framework",
					CoreVideo,
					"-framework",
					Foundation,
					"-framework",
					QuartzCore,
					"-framework",
					Security,
					"-framework",
					CoreGraphics,
					"-framework",
					CoreText,
					"-framework",
					CoreFoundation,
				);
				LD_RUNPATH_SEARCH_PATHS = "$(inherited) @executable_path/Frameworks";
				SDKROOT = iphoneos;
				TARGETED_DEVICE_FAMILY = "1,2";
			}};
			name = Debug;
		}};
		453B4F41EBA135F16129FB77 /* Debug */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
				ALWAYS_SEARCH_USER_PATHS = NO;
				CLANG_ANALYZER_NONNULL = YES;
				CLANG_ANALYZER_NUMBER_OBJECT_CONVERSION = YES_AGGRESSIVE;
				CLANG_CXX_LANGUAGE_STANDARD = "gnu++14";
				CLANG_CXX_LIBRARY = "libc++";
				CLANG_ENABLE_MODULES = YES;
				CLANG_ENABLE_OBJC_ARC = YES;
				CLANG_ENABLE_OBJC_WEAK = YES;
				CLANG_WARN_BLOCK_CAPTURE_AUTORELEASING = YES;
				CLANG_WARN_BOOL_CONVERSION = YES;
				CLANG_WARN_COMMA = YES;
				CLANG_WARN_CONSTANT_CONVERSION = YES;
				CLANG_WARN_DEPRECATED_OBJC_IMPLEMENTATIONS = YES;
				CLANG_WARN_DIRECT_OBJC_ISA_USAGE = YES_ERROR;
				CLANG_WARN_DOCUMENTATION_COMMENTS = YES;
				CLANG_WARN_EMPTY_BODY = YES;
				CLANG_WARN_ENUM_CONVERSION = YES;
				CLANG_WARN_INFINITE_RECURSION = YES;
				CLANG_WARN_INT_CONVERSION = YES;
				CLANG_WARN_NON_LITERAL_NULL_CONVERSION = YES;
				CLANG_WARN_OBJC_IMPLICIT_RETAIN_SELF = YES;
				CLANG_WARN_OBJC_LITERAL_CONVERSION = YES;
				CLANG_WARN_OBJC_ROOT_CLASS = YES_ERROR;
				CLANG_WARN_QUOTED_INCLUDE_IN_FRAMEWORK_HEADER = YES;
				CLANG_WARN_RANGE_LOOP_ANALYSIS = YES;
				CLANG_WARN_STRICT_PROTOTYPES = YES;
				CLANG_WARN_SUSPICIOUS_MOVE = YES;
				CLANG_WARN_UNGUARDED_AVAILABILITY = YES_AGGRESSIVE;
				CLANG_WARN_UNREACHABLE_CODE = YES;
				CLANG_WARN__DUPLICATE_METHOD_MATCH = YES;
				COPY_PHASE_STRIP = NO;
				DEBUG_INFORMATION_FORMAT = dwarf;
				ENABLE_STRICT_OBJC_MSGSEND = YES;
				ENABLE_TESTABILITY = YES;
				GCC_C_LANGUAGE_STANDARD = gnu11;
				GCC_DYNAMIC_NO_PIC = NO;
				GCC_NO_COMMON_BLOCKS = YES;
				GCC_OPTIMIZATION_LEVEL = 0;
				GCC_PREPROCESSOR_DEFINITIONS = (
					"DEBUG=1",
					"$(inherited)",
				);
				GCC_WARN_64_TO_32_BIT_CONVERSION = YES;
				GCC_WARN_ABOUT_RETURN_TYPE = YES_ERROR;
				GCC_WARN_UNDECLARED_SELECTOR = YES;
				GCC_WARN_UNINITIALIZED_AUTOS = YES_AGGRESSIVE;
				GCC_WARN_UNUSED_FUNCTION = YES;
				GCC_WARN_UNUSED_VARIABLE = YES;
				MTL_ENABLE_DEBUG_INFO = INCLUDE_SOURCE;
				MTL_FAST_MATH = YES;
				ONLY_ACTIVE_ARCH = YES;
				PRODUCT_NAME = "$(TARGET_NAME)";
				SWIFT_ACTIVE_COMPILATION_CONDITIONS = DEBUG;
				SWIFT_OPTIMIZATION_LEVEL = "-Onone";
				SWIFT_VERSION = 5.0;
			}};
			name = Debug;
		}};
		79AB7DD7EE74F541074453FD /* Release */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
				ALWAYS_SEARCH_USER_PATHS = NO;
				CLANG_ANALYZER_NONNULL = YES;
				CLANG_ANALYZER_NUMBER_OBJECT_CONVERSION = YES_AGGRESSIVE;
				CLANG_CXX_LANGUAGE_STANDARD = "gnu++14";
				CLANG_CXX_LIBRARY = "libc++";
				CLANG_ENABLE_MODULES = YES;
				CLANG_ENABLE_OBJC_ARC = YES;
				CLANG_ENABLE_OBJC_WEAK = YES;
				CLANG_WARN_BLOCK_CAPTURE_AUTORELEASING = YES;
				CLANG_WARN_BOOL_CONVERSION = YES;
				CLANG_WARN_COMMA = YES;
				CLANG_WARN_CONSTANT_CONVERSION = YES;
				CLANG_WARN_DEPRECATED_OBJC_IMPLEMENTATIONS = YES;
				CLANG_WARN_DIRECT_OBJC_ISA_USAGE = YES_ERROR;
				CLANG_WARN_DOCUMENTATION_COMMENTS = YES;
				CLANG_WARN_EMPTY_BODY = YES;
				CLANG_WARN_ENUM_CONVERSION = YES;
				CLANG_WARN_INFINITE_RECURSION = YES;
				CLANG_WARN_INT_CONVERSION = YES;
				CLANG_WARN_NON_LITERAL_NULL_CONVERSION = YES;
				CLANG_WARN_OBJC_IMPLICIT_RETAIN_SELF = YES;
				CLANG_WARN_OBJC_LITERAL_CONVERSION = YES;
				CLANG_WARN_OBJC_ROOT_CLASS = YES_ERROR;
				CLANG_WARN_QUOTED_INCLUDE_IN_FRAMEWORK_HEADER = YES;
				CLANG_WARN_RANGE_LOOP_ANALYSIS = YES;
				CLANG_WARN_STRICT_PROTOTYPES = YES;
				CLANG_WARN_SUSPICIOUS_MOVE = YES;
				CLANG_WARN_UNGUARDED_AVAILABILITY = YES_AGGRESSIVE;
				CLANG_WARN_UNREACHABLE_CODE = YES;
				CLANG_WARN__DUPLICATE_METHOD_MATCH = YES;
				COPY_PHASE_STRIP = NO;
				DEBUG_INFORMATION_FORMAT = "dwarf-with-dsym";
				ENABLE_NS_ASSERTIONS = NO;
				ENABLE_STRICT_OBJC_MSGSEND = YES;
				GCC_C_LANGUAGE_STANDARD = gnu11;
				GCC_NO_COMMON_BLOCKS = YES;
				GCC_WARN_64_TO_32_BIT_CONVERSION = YES;
				GCC_WARN_ABOUT_RETURN_TYPE = YES_ERROR;
				GCC_WARN_UNDECLARED_SELECTOR = YES;
				GCC_WARN_UNINITIALIZED_AUTOS = YES_AGGRESSIVE;
				GCC_WARN_UNUSED_FUNCTION = YES;
				GCC_WARN_UNUSED_VARIABLE = YES;
				MTL_ENABLE_DEBUG_INFO = NO;
				MTL_FAST_MATH = YES;
				PRODUCT_NAME = "$(TARGET_NAME)";
				SWIFT_COMPILATION_MODE = wholemodule;
				SWIFT_OPTIMIZATION_LEVEL = "-O";
				SWIFT_VERSION = 5.0;
			}};
			name = Release;
		}};
		BECACC829D12230529BB85F1 /* Release */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
				ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon;
				ASSETCATALOG_COMPILER_GLOBAL_ACCENT_COLOR_NAME = AccentColor;
				GENERATE_INFOPLIST_FILE = NO;
				INFOPLIST_FILE = "{project_name}/Info.plist";
				OTHER_LDFLAGS = (
					"-L$(SRCROOT)/../staticlib/ios",
					"-l{project_name_lib}",
					"-lc++",
					"-framework",
					UIKit,
					"-framework",
					Metal,
					"-framework",
					CoreVideo,
					"-framework",
					Foundation,
					"-framework",
					QuartzCore,
					"-framework",
					Security,
					"-framework",
					CoreGraphics,
					"-framework",
					CoreText,
					"-framework",
					CoreFoundation,
				);
				LD_RUNPATH_SEARCH_PATHS = "$(inherited) @executable_path/Frameworks";
				SDKROOT = iphoneos;
				TARGETED_DEVICE_FAMILY = "1,2";
				VALIDATE_PRODUCT = YES;
			}};
			name = Release;
		}};
/* End XCBuildConfiguration section */

/* Begin XCConfigurationList section */
		2387A685D112612CDD6DD78D /* Build configuration list for PBXProject "{project_name}" */ = {{
			isa = XCConfigurationList;
			buildConfigurations = (
				453B4F41EBA135F16129FB77 /* Debug */,
				79AB7DD7EE74F541074453FD /* Release */,
			);
			defaultConfigurationIsVisible = 0;
			defaultConfigurationName = Release;
		}};
		E78EF380B86DAD433C3C7F9E /* Build configuration list for PBXNativeTarget "{project_name}" */ = {{
			isa = XCConfigurationList;
			buildConfigurations = (
				BECACC829D12230529BB85F1 /* Release */,
				22844781F1EDECBF9F17C0BA /* Debug */,
			);
			defaultConfigurationIsVisible = 0;
			defaultConfigurationName = Release;
		}};
/* End XCConfigurationList section */
	}};
	rootObject = F745F1A60BF1B4F8EBEABC66 /* Project object */;
}}
"#,
            project_name = project_name,
            project_name_lib = project_name_lib
        ),
    )
    .unwrap();

    let app_dir = ios_dir.join(project_name);
    fs::create_dir_all(&app_dir).unwrap();
    
    fs::write(
        app_dir.join("Info.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>$(EXECUTABLE_NAME)</string>
    <key>CFBundleIdentifier</key>
    <string>com.example.app</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$(PRODUCT_NAME)</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSRequiresIPhoneOS</key>
    <true/>
    <key>UILaunchScreen</key>
    <dict/>
    <key>UIRequiresFullScreen</key>
    <true/>
    <key>UISupportedInterfaceOrientations</key>
    <array>
        <string>UIInterfaceOrientationPortrait</string>
        <string>UIInterfaceOrientationLandscapeLeft</string>
        <string>UIInterfaceOrientationLandscapeRight</string>
    </array>
</dict>
</plist>"#,
    ).unwrap();

    fs::write(
        app_dir.join("main.swift"),
        r#"import Foundation

@_silgen_name("__generated_entrance_point")
func __generated_entrance_point()

print("Calling Rust...")
__generated_entrance_point()
"#,
    ).unwrap();
}
