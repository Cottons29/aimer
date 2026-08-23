import Darwin
import UIKit

@_silgen_name("aimer_wasm_device_proof")
func aimer_wasm_device_proof() -> UInt32

final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        let result = aimer_wasm_device_proof()
        print("AIMER_WASM_DEVICE_PROOF_RESULT=\(result)")
        fflush(stdout)
        exit(result == 0 ? EXIT_SUCCESS : EXIT_FAILURE)
    }
}

UIApplicationMain(
    CommandLine.argc,
    CommandLine.unsafeArgv,
    nil,
    NSStringFromClass(AppDelegate.self)
)