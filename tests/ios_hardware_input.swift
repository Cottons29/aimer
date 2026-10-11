// Runs the production Swift host in a small simulator app. The Rust ABI is
// recorded here; Rust-side dispatch is covered by aimer_quiver's headless tests.
import UIKit

private var pointerEvents: [(UInt32, Double, Double, UInt32)] = []
private var scrollEvents: [(Double, Double, UInt32, UInt64)] = []
private var keyEvents: [(UInt32, UInt32, UInt64, String)] = []

@_cdecl("aimer_ios_pointer_event")
func recordPointer(_ kind: UInt32, _ x: Double, _ y: Double, _ button: UInt32, _ flags: UInt64) {
    pointerEvents.append((kind, x, y, button))
}
@_cdecl("aimer_ios_scroll_event")
func recordScroll(_ x: Double, _ y: Double, _ phase: UInt32, _ flags: UInt64) {
    scrollEvents.append((x, y, phase, flags))
}
@_cdecl("aimer_ios_key_event")
func recordKey(_ usage: UInt32, _ action: UInt32, _ flags: UInt64, _ ptr: UnsafePointer<UInt8>?, _ len: Int) {
    let text = ptr.map { String(decoding: UnsafeBufferPointer(start: $0, count: len), as: UTF8.self) } ?? ""
    keyEvents.append((usage, action, flags, text))
}
@_cdecl("trigger_rust_backspace") func recordBackspace() {}
@_cdecl("trigger_rust_insert_text") func recordText(_ ptr: UnsafePointer<UInt8>, _ len: Int) {}
@_cdecl("trigger_rust_set_marked_text") func recordPreedit(_ ptr: UnsafePointer<UInt8>, _ len: Int) {}
@_cdecl("trigger_rust_text_editing_delta")
func recordDelta(_ session: UInt64, _ revision: UInt64, _ start: Int, _ end: Int,
    _ ptr: UnsafePointer<UInt8>?, _ len: Int, _ selectionStart: Int, _ selectionEnd: Int,
    _ composingStart: Int, _ composingEnd: Int) {}
@_cdecl("aimer_ios_frame_tick") func recordTick() {}

private class TestKey: UIKey {
    var usage: UIKeyboardHIDUsage = .keyboardA
    var flags: UIKeyModifierFlags = []
    var text = "a"
    override var keyCode: UIKeyboardHIDUsage { usage }
    override var modifierFlags: UIKeyModifierFlags { flags }
    override var characters: String { text }
    override var charactersIgnoringModifiers: String { text }
}
private class TestPress: UIPress {
    var testKey = TestKey()
    override var key: UIKey? { testKey }
}
private class TestTouch: UITouch {
    var touchType: UITouch.TouchType = .indirectPointer
    var touchPhase: UITouch.Phase = .began
    override var type: UITouch.TouchType { touchType }
    override var phase: UITouch.Phase { touchPhase }
    override func location(in view: UIView?) -> CGPoint { CGPoint(x: 23.25, y: 42.5) }
}
private class TestEvent: UIEvent {
    var touches: Set<UITouch> = []
    var mask: UIEvent.ButtonMask = .primary
    override var allTouches: Set<UITouch>? { touches }
    override var buttonMask: UIEvent.ButtonMask { mask }
}

// Scroll recognizers have no touches and expose a virtual pan location that
// moves with the translation. It must never replace the actual hover position.
private class TestScroll: UIPanGestureRecognizer {
    var testState: UIGestureRecognizer.State = .began
    var delta = CGPoint.zero
    var virtualLocation = CGPoint.zero
    var releaseVelocity = CGPoint.zero
    override func velocity(in view: UIView?) -> CGPoint { releaseVelocity }
    override var state: UIGestureRecognizer.State {
        get { testState }
        set { testState = newValue }
    }
    override func location(in view: UIView?) -> CGPoint { virtualLocation }
    override func translation(in view: UIView?) -> CGPoint { delta }
    override func setTranslation(_ translation: CGPoint, in view: UIView?) { delta = translation }
}

private func verifyScrollMomentum() {
    var momentum = HardwareScrollMomentum()
    precondition(momentum.start(velocity: CGPoint(x: 0, y: -1200), at: 10),
        "releasing a moving scroll must start a glide")
    precondition(momentum.step(at: 10) == nil, "a repeated vsync cannot move twice")
    var total: CGFloat = 0
    var previous: CGFloat = .infinity
    var frames = 0
    for frame in 1...600 {
        if let delta = momentum.step(at: 10 + Double(frame) / 120) {
            precondition(delta.x == 0 && delta.y < 0)
            precondition(abs(delta.y) <= previous + 0.00001, "the release glide must decelerate")
            previous = abs(delta.y)
            total += delta.y
            frames += 1
        }
        if !momentum.isActive { break }
    }
    precondition(frames > 30 && frames < 600 && total < -100, "glide must continue after input stops and settle")
    precondition(total < -800 && total > -950, "a 1200-point/s release needs the stronger 1.5x projection")
    precondition(momentum.step(at: 20) == nil, "settled glides must stay idle")
    for velocity in [CGPoint.zero, CGPoint(x: 0, y: 10), CGPoint(x: CGFloat.nan, y: 1),
        CGPoint(x: 0, y: CGFloat.infinity)] {
        precondition(!momentum.start(velocity: velocity, at: 30), "stopped/invalid input must not fling")
    }
    precondition(!momentum.start(velocity: CGPoint(x: 0, y: 1200), at: .nan))
    precondition(momentum.start(velocity: CGPoint(x: 0, y: 1200), at: 40))
    precondition(momentum.step(at: 39) == nil, "out-of-order ticks must not add distance")
    momentum.stop()
    precondition(!momentum.isActive && momentum.step(at: 41) == nil, "new input must cancel a glide")
    precondition(momentum.start(velocity: CGPoint(x: 0, y: -1e9), at: 50))
    let bounded = momentum.step(at: 50 + 1.0 / 120)!
    precondition(bounded.y > -51 && bounded.y < 0, "extreme release velocities must stay bounded")
    momentum.stop()
}

private func verifyInput(_ window: UIWindow) {
    verifyScrollMomentum()
    KeyboardForwarder.shared.installIfPossible()
    guard let view = HardwareInputForwarder.shared.view,
        let editor = KeyboardForwarder.shared.inputView else { fatalError("host failed to install input views") }
    precondition(view.isFirstResponder, "hardware fallback must receive keys without showing the software keyboard")
    let touch = TestTouch()
    let event = TestEvent()
    event.touches = [touch]
    let p = CGPoint(x: 23.25, y: 42.5)
    precondition(window.hitTest(p, with: event) === view, "mouse contact must avoid winit's synthetic touch path")
    touch.touchType = .direct
    precondition(window.hitTest(p, with: event) !== view, "finger touches must pass through")
    touch.touchType = .pencil
    precondition(window.hitTest(p, with: event) !== view, "Pencil contacts must pass through")
    touch.touchType = .indirectPointer
    pointerEvents.removeAll()
    view.touchesBegan([touch], with: event)
    event.mask = [.primary, .secondary]
    view.touchesMoved([touch], with: event)
    event.mask = .secondary
    view.touchesMoved([touch], with: event)
    touch.touchPhase = .ended
    event.mask = []
    view.touchesEnded([touch], with: event)
    let changes = pointerEvents.filter { $0.0 == 2 || $0.0 == 3 }
    precondition(changes.count == 4, "each button transition must be forwarded once")
    precondition(changes[0].0 == 2 && changes[0].3 == 0)
    precondition(changes[1].0 == 2 && changes[1].3 == 1)
    precondition(changes[2].0 == 3 && changes[2].3 == 0)
    precondition(changes[3].0 == 3 && changes[3].3 == 1)
    precondition(changes.allSatisfy { $0.1 == p.x && $0.2 == p.y }, "UIKit points must stay logical")

    touch.touchPhase = .began
    event.mask = .primary
    view.touchesBegan([touch], with: event)
    pointerEvents.removeAll()
    view.touchesCancelled([touch], with: event)
    precondition(pointerEvents.contains { $0.0 == 4 }, "interrupted contacts must cancel the gesture")
    precondition(!pointerEvents.contains { $0.0 == 3 }, "cancellation must not activate a click")

    let scroll = TestScroll()
    scrollEvents.removeAll()
    pointerEvents.removeAll()
    for (state, delta) in [(UIGestureRecognizer.State.began, CGPoint(x: 0, y: -3)),
        (.changed, CGPoint(x: 1.25, y: -15.5)), (.ended, CGPoint.zero)] {
        scroll.testState = state
        scroll.delta = delta
        scroll.virtualLocation = CGPoint(x: p.x + 500, y: p.y - 500)
        view.scroll(scroll)
        precondition(scroll.delta == .zero, "each incremental delta must be drained exactly once")
    }
    precondition(scrollEvents.map { $0.2 } == [0, 1, 2], "trackpad phases must reach Rust")
    precondition(scrollEvents[1].0 == 1.25 && scrollEvents[1].1 == -15.5, "preserve fractional scroll points")
    precondition(pointerEvents.filter { $0.0 == 0 }.allSatisfy { $0.1 == p.x && $0.2 == p.y },
        "scrolling must stay at the hover position instead of moving hit testing with the virtual pan")
    scroll.testState = .cancelled
    view.scroll(scroll)
    precondition(scrollEvents.last?.2 == 3, "interrupted trackpad gestures must terminate")
    scroll.testState = .possible
    view.scroll(scroll)
    precondition(scrollEvents.count == 4, "unrecognized gestures must not scroll")

    // Device scroll axes stay horizontal/vertical when the window rotates.
    // Exercise the real UIKit conversion with a rotated, translated window.
    let originalTransform = window.transform
    let fixed = window.screen.fixedCoordinateSpace
    for angle in [CGFloat.pi / 2, -.pi / 2, .pi, 0] {
        window.transform = CGAffineTransform(rotationAngle: angle).translatedBy(x: 17, y: -23)
        let origin = window.convert(CGPoint.zero, from: fixed)
        let expected = CGPoint(x: 1.25, y: -15.5)
        let endpoint = window.convert(expected, from: fixed)
        scroll.delta = CGPoint(x: endpoint.x - origin.x, y: endpoint.y - origin.y)
        scroll.testState = .changed
        view.scroll(scroll)
        let reported = scrollEvents.last!
        precondition(abs(reported.0 - expected.x) < 0.0001 && abs(reported.1 - expected.y) < 0.0001,
            "window rotation must not turn a vertical wheel or trackpad scroll into horizontal input")
    }
    window.transform = originalTransform

    scroll.releaseVelocity = CGPoint(x: 0, y: -1200)
    scroll.testState = .began
    scroll.delta = CGPoint(x: 0, y: -10)
    view.forwardScroll(scroll, at: 10)
    scroll.testState = .ended
    scroll.delta = .zero
    view.forwardScroll(scroll, at: 10.02)
    precondition(view.hasScrollMomentum, "the UIKit release must activate the display-link glide")
    precondition(scrollEvents.suffix(2).map { $0.2 } == [2, 0], "momentum must start after contact ended")
    view.advanceScrollMomentum(at: 10.02 + 1.0 / 120)
    precondition(scrollEvents.last!.1 < 0 && scrollEvents.last!.2 == 1, "vsync must deliver release movement")
    touch.touchType = .direct
    touch.touchPhase = .began
    precondition(view.hitTest(p, with: event) == nil, "a finger must still reach winit")
    precondition(!view.hasScrollMomentum && scrollEvents.last!.2 == 3, "a new finger contact must interrupt glide")
    touch.touchType = .indirectPointer
    scroll.testState = .began
    scroll.delta = CGPoint(x: 0, y: -10)
    view.forwardScroll(scroll, at: 20)
    scroll.testState = .ended
    scroll.delta = .zero
    view.forwardScroll(scroll, at: 20.02)
    precondition(view.hasScrollMomentum)
    scroll.testState = .began
    view.forwardScroll(scroll, at: 20.03)
    precondition(!view.hasScrollMomentum, "a new wheel or trackpad gesture must interrupt the previous glide")
    scroll.testState = .ended
    view.forwardScroll(scroll, at: 22)
    precondition(!view.hasScrollMomentum, "pausing before release must not launch a stale velocity")

    let press = TestPress()
    keyEvents.removeAll()
    view.pressesBegan([press], with: nil)
    view.pressesChanged([press], with: nil)
    view.pressesEnded([press], with: nil)
    precondition(keyEvents.map { $0.1 } == [0, 2, 1], "press/repeat/release must remain distinct")
    precondition(keyEvents.allSatisfy { $0.0 == 4 && $0.3 == "a" })
    keyEvents.removeAll()
    let unhandled = HardwareInputForwarder.shared.forward([press], action: 0, textInput: true)
    precondition(unhandled == [press], "printable keys must remain available to UITextInput")
    let composing = HardwareInputForwarder.shared.forward([press], action: 0, textInput: true, composing: true)
    precondition(composing == [press] && keyEvents.count == 1, "IME keys must not reach Rust twice")
    HardwareInputForwarder.shared.releaseKeys()
    press.testKey.usage = .keyboardLeftArrow
    press.testKey.text = ""
    precondition(HardwareInputForwarder.shared.forward([press], action: 0, textInput: true).isEmpty,
        "Rust must own navigation instead of moving the mirrored editor a second time")
    HardwareInputForwarder.shared.releaseKeys()

    KeyboardForwarder.shared.showKeyboard()
    precondition(editor.isFirstResponder, "text editing must retain the native IME responder")
    keyEvents.removeAll()
    let command = editor.keyCommands!.first { $0.input == "c" }!
    precondition(editor.responds(to: command.action))
    _ = editor.perform(command.action, with: command)
    precondition(keyEvents.count == 2 && keyEvents[0].3 == "c" && keyEvents[0].2 == UInt64(UIKeyModifierFlags.command.rawValue))
    KeyboardForwarder.shared.dismissKeyboard()
    precondition(view.isFirstResponder, "dismissal must restore hardware keyboard input")
    print("AIMER_UIKIT_INPUT_TESTS_PASSED")
    fflush(stdout)
    exit(0)
}

class InputTestScene: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?
    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
        guard let scene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        self.window = window
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { verifyInput(window) }
    }
}
class InputTestApplication: UIResponder, UIApplicationDelegate {
    func application(_ application: UIApplication, configurationForConnecting session: UISceneSession,
        options: UIScene.ConnectionOptions) -> UISceneConfiguration {
        let config = UISceneConfiguration(name: "Input tests", sessionRole: session.role)
        config.delegateClass = InputTestScene.self
        return config
    }
}
@_cdecl("__generated_entrance_point")
func testEntrance() {
    _ = UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(InputTestApplication.self))
}
