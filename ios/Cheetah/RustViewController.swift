import UIKit

final class RustViewController: UIViewController {
    private var host: RustHost?
    private var keyboardObserver: NSObjectProtocol?
    private var lifecycleObservers: [NSObjectProtocol] = []

    override var preferredStatusBarStyle: UIStatusBarStyle {
        .lightContent
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        overrideUserInterfaceStyle = .dark
        view.backgroundColor = UIColor(hex: "#313338")

        let container = UIView()
        container.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(container)

        NSLayoutConstraint.activate([
            container.topAnchor.constraint(equalTo: view.topAnchor),
            container.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            container.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            container.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])

        host = RustHost(container: container)

        keyboardObserver = NotificationCenter.default.addObserver(
            forName: UIResponder.keyboardWillChangeFrameNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.view.layoutIfNeeded()
            self?.updateHost()
        }

        lifecycleObservers = [
            NotificationCenter.default.addObserver(
                forName: UIApplication.didEnterBackgroundNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                self?.host?.setActive(false)
            },
            NotificationCenter.default.addObserver(
                forName: UIApplication.willEnterForegroundNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                guard let self else {
                    return
                }
                self.view.layoutIfNeeded()
                self.updateHost()
                self.host?.setActive(true)
            },
        ]
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        updateHost()
    }

    private func updateHost() {
        guard let host else {
            return
        }

        let keyboardFrame = view.keyboardLayoutGuide.layoutFrame
        let keyboardInset: CGFloat
        if keyboardFrame.minY > 0, keyboardFrame.minY < view.bounds.maxY {
            keyboardInset = max(0, view.bounds.maxY - keyboardFrame.minY)
        } else {
            keyboardInset = 0
        }

        let glass: Bool
        #if compiler(>=6.2)
        if #available(iOS 26.0, *) {
            glass = true
        } else {
            glass = false
        }
        #else
        glass = false
        #endif

        host.updateHostInfo(
            safeTop: view.safeAreaInsets.top,
            safeBottom: view.safeAreaInsets.bottom,
            keyboardInset: keyboardInset,
            glass: glass
        )

        let size = host.containerSize
        if size.width > 0, size.height > 0 {
            host.resize(to: size)
        }
    }

    deinit {
        if let keyboardObserver {
            NotificationCenter.default.removeObserver(keyboardObserver)
        }
        for observer in lifecycleObservers {
            NotificationCenter.default.removeObserver(observer)
        }
        host?.dispose()
    }
}
