import Darwin
import Foundation
import UIKit

@_silgen_name("aimer_reload_transport_proof_start")
func aimer_reload_transport_proof_start() -> UInt16

final class AppDelegate: UIResponder, UIApplicationDelegate, NetServiceDelegate {
    private var service: NetService?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        let port = aimer_reload_transport_proof_start()
        let serviceName = ProcessInfo.processInfo.environment["AIMER_RELOAD_SERVICE_NAME"]
        NSLog(
            "Aimer reload proof startup: listenerPort=%u serviceNamePresent=%@",
            port,
            serviceName == nil ? "false" : "true"
        )
        guard port > 1024,
              let serviceName
        else {
            print("AIMER_RELOAD_TRANSPORT_DEVICE_PROOF_STARTUP_STATUS=\(port)")
            fflush(stdout)
            return true
        }
        let service = NetService(
            domain: "local.",
            type: "_aimer-reload._tcp.",
            name: serviceName,
            port: Int32(port)
        )
        service.includesPeerToPeer = true
        service.delegate = self
        service.publish()
        self.service = service
        return true
    }

    func netService(_ sender: NetService, didNotPublish errorDict: [String: NSNumber]) {
        print("AIMER_RELOAD_TRANSPORT_DEVICE_PROOF_RESULT=3")
        fflush(stdout)
        exit(EXIT_FAILURE)
    }
}

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        guard let windowScene = scene as? UIWindowScene else {
            return
        }
        let window = UIWindow(windowScene: windowScene)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        self.window = window
    }
}

UIApplicationMain(
    CommandLine.argc,
    CommandLine.unsafeArgv,
    nil,
    NSStringFromClass(AppDelegate.self)
)