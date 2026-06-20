import AppKit
import CBrowser

// MARK: - DevTools

/// One parsed row of the engine's network log JSON.
private struct NetRow {
    var method: String
    var url: String
    var status: Int
    var ok: Bool
    var ms: Double
    var size: Int
    var type: String

    /// "Name" column: the last non-empty path segment, or the host for "/".
    var name: String {
        guard let u = URL(string: url) else { return url }
        let segs = u.path.split(separator: "/").map(String.init)
        if let last = segs.last, !last.isEmpty { return last }
        return u.host ?? url
    }

    var typeShort: String {
        String(type.split(separator: ";").first ?? Substring(type))
            .trimmingCharacters(in: .whitespaces)
    }

    var statusText: String { status == 0 ? "(failed)" : String(status) }

    var sizeText: String {
        let bytes = Double(size)
        if size <= 0 { return "—" }
        if bytes < 1024 { return "\(size) B" }
        if bytes < 1024 * 1024 { return String(format: "%.1f KB", bytes / 1024) }
        return String(format: "%.1f MB", bytes / (1024 * 1024))
    }

    var timeText: String { "\(Int(ms.rounded())) ms" }
}

/// The bottom devtools panel: a Console tab (page console output + a REPL) and a Network tab
/// (a request table). Hidden by default; toggled with ⌘⌥I. Reaches the active tab's engine and
/// the app's refresh() via injected closures.
/// A thin draggable strip (top edge of the DevTools panel) that reports vertical drag deltas in
/// window points; positive delta = dragged UP = the panel should grow taller.
final class ResizeHandle: NSView {
    var onDrag: ((CGFloat) -> Void)?
    private var lastY: CGFloat?
    override func resetCursorRects() { addCursorRect(bounds, cursor: .resizeUpDown) }
    override func mouseDown(with event: NSEvent) { lastY = event.locationInWindow.y }
    override func mouseDragged(with event: NSEvent) {
        guard let l = lastY else { return }
        let y = event.locationInWindow.y
        onDrag?(y - l) // window coords are bottom-origin: dragging up increases y → grow
        lastY = y
    }
    override func mouseUp(with event: NSEvent) { lastY = nil }
}

final class DevToolsView: NSView {
    /// Returns the active tab's engine, or nil if there is none / a load is in flight.
    var engineProvider: (() -> OpaquePointer?)?
    /// Ask the app to re-render the page (an eval may have mutated the DOM).
    var onRefreshPage: (() -> Void)?
    /// Close the panel (the ✕ button).
    var onCloseDevTools: (() -> Void)?
    /// Drag on the top resize handle: `delta` points (positive = make the panel taller).
    var onResizeDrag: ((CGFloat) -> Void)?

    private let closeButton = NSButton(title: "✕", target: nil, action: nil)

    /// REPL input/output lines, kept Swift-side so they survive console-text refreshes. Cleared
    /// on navigation (a REPL session is per page).
    private var replLines: [String] = []

    private let segmented = NSSegmentedControl(labels: ["Console", "Network", "Elements"], trackingMode: .selectOne, target: nil, action: nil)
    private let header = NSTextField(labelWithString: "")

    // Console tab
    private let consoleScroll = NSScrollView()
    private let consoleText = NSTextView()
    private let promptLabel = NSTextField(labelWithString: "›")
    private let replField = NSTextField()
    private var consoleContainer = NSView()

    // Network tab
    private let netScroll = NSScrollView()
    private let netTable = NSTableView()
    private var netContainer = NSView()
    private var netRows: [NetRow] = []

    // Elements tab
    private let elementsScroll = NSScrollView()
    private let elementsOutline = NSOutlineView()
    private var elementsContainer = NSView()
    /// Root of the parsed DOM tree shown in the outline (nil until first built).
    private var domRoot: DOMNode?
    /// Snapshot of the DOM tree JSON the current `domRoot` was built from, to skip rebuilds when the
    /// page hasn't changed.
    private var domTreeJSON: String = ""

    private static let bg = NSColor(calibratedRed: 0.10, green: 0.10, blue: 0.11, alpha: 1.0)
    private static let mono = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = DevToolsView.bg.cgColor
        buildUI()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override var isFlipped: Bool { true }

    private func buildUI() {
        // Thin top divider.
        let divider = NSBox()
        divider.boxType = .custom
        divider.borderWidth = 0
        divider.fillColor = NSColor.separatorColor
        divider.translatesAutoresizingMaskIntoConstraints = false
        addSubview(divider)

        // Draggable resize handle across the top edge (resize-up-down cursor).
        let resize = ResizeHandle()
        resize.onDrag = { [weak self] delta in self?.onResizeDrag?(delta) }
        resize.translatesAutoresizingMaskIntoConstraints = false
        addSubview(resize)

        // Close (✕) button, top-right.
        closeButton.bezelStyle = .inline
        closeButton.isBordered = false
        closeButton.font = NSFont.systemFont(ofSize: 13)
        closeButton.contentTintColor = NSColor.secondaryLabelColor
        closeButton.target = self
        closeButton.action = #selector(closeDevToolsClicked)
        closeButton.translatesAutoresizingMaskIntoConstraints = false
        addSubview(closeButton)

        // Tab switcher + header count.
        segmented.selectedSegment = 0
        segmented.target = self
        segmented.action = #selector(tabChanged)
        segmented.translatesAutoresizingMaskIntoConstraints = false
        addSubview(segmented)

        header.font = NSFont.systemFont(ofSize: 10)
        header.textColor = NSColor.secondaryLabelColor
        header.translatesAutoresizingMaskIntoConstraints = false
        addSubview(header)

        buildConsoleTab()
        buildNetworkTab()
        buildElementsTab()

        NSLayoutConstraint.activate([
            divider.topAnchor.constraint(equalTo: topAnchor),
            divider.leadingAnchor.constraint(equalTo: leadingAnchor),
            divider.trailingAnchor.constraint(equalTo: trailingAnchor),
            divider.heightAnchor.constraint(equalToConstant: 1),

            resize.topAnchor.constraint(equalTo: topAnchor),
            resize.leadingAnchor.constraint(equalTo: leadingAnchor),
            resize.trailingAnchor.constraint(equalTo: trailingAnchor),
            resize.heightAnchor.constraint(equalToConstant: 6),

            segmented.topAnchor.constraint(equalTo: divider.bottomAnchor, constant: 6),
            segmented.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),

            closeButton.centerYAnchor.constraint(equalTo: segmented.centerYAnchor),
            closeButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            closeButton.widthAnchor.constraint(equalToConstant: 22),

            header.centerYAnchor.constraint(equalTo: segmented.centerYAnchor),
            header.trailingAnchor.constraint(equalTo: closeButton.leadingAnchor, constant: -10),

            consoleContainer.topAnchor.constraint(equalTo: segmented.bottomAnchor, constant: 6),
            consoleContainer.leadingAnchor.constraint(equalTo: leadingAnchor),
            consoleContainer.trailingAnchor.constraint(equalTo: trailingAnchor),
            consoleContainer.bottomAnchor.constraint(equalTo: bottomAnchor),

            netContainer.topAnchor.constraint(equalTo: segmented.bottomAnchor, constant: 6),
            netContainer.leadingAnchor.constraint(equalTo: leadingAnchor),
            netContainer.trailingAnchor.constraint(equalTo: trailingAnchor),
            netContainer.bottomAnchor.constraint(equalTo: bottomAnchor),

            elementsContainer.topAnchor.constraint(equalTo: segmented.bottomAnchor, constant: 6),
            elementsContainer.leadingAnchor.constraint(equalTo: leadingAnchor),
            elementsContainer.trailingAnchor.constraint(equalTo: trailingAnchor),
            elementsContainer.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])

        showTab(0)
    }

    private func buildConsoleTab() {
        consoleContainer.translatesAutoresizingMaskIntoConstraints = false
        addSubview(consoleContainer)

        consoleText.isEditable = false
        consoleText.isSelectable = true
        consoleText.drawsBackground = true
        consoleText.backgroundColor = DevToolsView.bg
        consoleText.textColor = NSColor(white: 0.85, alpha: 1.0)
        consoleText.font = DevToolsView.mono
        consoleText.textContainerInset = NSSize(width: 6, height: 4)
        consoleText.isVerticallyResizable = true
        consoleText.isHorizontallyResizable = false
        consoleText.autoresizingMask = [.width]
        consoleText.textContainer?.widthTracksTextView = true

        consoleScroll.documentView = consoleText
        consoleScroll.hasVerticalScroller = true
        consoleScroll.drawsBackground = true
        consoleScroll.backgroundColor = DevToolsView.bg
        consoleScroll.translatesAutoresizingMaskIntoConstraints = false
        consoleContainer.addSubview(consoleScroll)

        promptLabel.font = DevToolsView.mono
        promptLabel.textColor = NSColor(calibratedRed: 0.5, green: 0.8, blue: 1.0, alpha: 1.0)
        promptLabel.translatesAutoresizingMaskIntoConstraints = false
        consoleContainer.addSubview(promptLabel)

        replField.isBezeled = false
        replField.isBordered = false
        replField.drawsBackground = false
        replField.focusRingType = .none
        replField.font = DevToolsView.mono
        replField.textColor = NSColor(white: 0.95, alpha: 1.0)
        replField.placeholderString = "Evaluate JavaScript in the page…"
        replField.usesSingleLineMode = true
        replField.cell?.usesSingleLineMode = true
        replField.target = self
        replField.action = #selector(replSubmit)
        replField.translatesAutoresizingMaskIntoConstraints = false
        consoleContainer.addSubview(replField)

        NSLayoutConstraint.activate([
            consoleScroll.topAnchor.constraint(equalTo: consoleContainer.topAnchor),
            consoleScroll.leadingAnchor.constraint(equalTo: consoleContainer.leadingAnchor),
            consoleScroll.trailingAnchor.constraint(equalTo: consoleContainer.trailingAnchor),

            promptLabel.leadingAnchor.constraint(equalTo: consoleContainer.leadingAnchor, constant: 8),
            promptLabel.bottomAnchor.constraint(equalTo: consoleContainer.bottomAnchor, constant: -6),

            replField.leadingAnchor.constraint(equalTo: promptLabel.trailingAnchor, constant: 6),
            replField.trailingAnchor.constraint(equalTo: consoleContainer.trailingAnchor, constant: -8),
            replField.centerYAnchor.constraint(equalTo: promptLabel.centerYAnchor),

            consoleScroll.bottomAnchor.constraint(equalTo: replField.topAnchor, constant: -6),
        ])
    }

    private func buildNetworkTab() {
        netContainer.translatesAutoresizingMaskIntoConstraints = false
        addSubview(netContainer)

        let cols: [(String, String, CGFloat)] = [
            ("name", "Name", 220),
            ("method", "Method", 70),
            ("status", "Status", 70),
            ("type", "Type", 130),
            ("size", "Size", 80),
            ("time", "Time", 80),
        ]
        for (id, title, width) in cols {
            let col = NSTableColumn(identifier: NSUserInterfaceItemIdentifier(id))
            col.title = title
            col.width = width
            col.minWidth = 40
            netTable.addTableColumn(col)
        }
        netTable.dataSource = self
        netTable.delegate = self
        netTable.usesAlternatingRowBackgroundColors = false
        netTable.backgroundColor = DevToolsView.bg
        netTable.gridColor = NSColor(white: 0.2, alpha: 1.0)
        netTable.gridStyleMask = [.solidHorizontalGridLineMask]
        netTable.rowHeight = 18
        netTable.headerView?.wantsLayer = true

        netScroll.documentView = netTable
        netScroll.hasVerticalScroller = true
        netScroll.drawsBackground = true
        netScroll.backgroundColor = DevToolsView.bg
        netScroll.translatesAutoresizingMaskIntoConstraints = false
        netContainer.addSubview(netScroll)

        NSLayoutConstraint.activate([
            netScroll.topAnchor.constraint(equalTo: netContainer.topAnchor),
            netScroll.leadingAnchor.constraint(equalTo: netContainer.leadingAnchor),
            netScroll.trailingAnchor.constraint(equalTo: netContainer.trailingAnchor),
            netScroll.bottomAnchor.constraint(equalTo: netContainer.bottomAnchor),
        ])
    }

    private func buildElementsTab() {
        elementsContainer.translatesAutoresizingMaskIntoConstraints = false
        addSubview(elementsContainer)

        let col = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("node"))
        col.title = "DOM"
        col.resizingMask = [.autoresizingMask]
        elementsOutline.addTableColumn(col)
        elementsOutline.outlineTableColumn = col
        elementsOutline.headerView = nil
        elementsOutline.dataSource = self
        elementsOutline.delegate = self
        elementsOutline.backgroundColor = DevToolsView.bg
        elementsOutline.usesAlternatingRowBackgroundColors = false
        elementsOutline.rowHeight = 18
        elementsOutline.indentationPerLevel = 14
        elementsOutline.autoresizesOutlineColumn = false
        elementsOutline.action = #selector(elementsRowClicked)
        elementsOutline.target = self

        elementsScroll.documentView = elementsOutline
        elementsScroll.hasVerticalScroller = true
        elementsScroll.hasHorizontalScroller = true
        elementsScroll.drawsBackground = true
        elementsScroll.backgroundColor = DevToolsView.bg
        elementsScroll.translatesAutoresizingMaskIntoConstraints = false
        elementsContainer.addSubview(elementsScroll)

        NSLayoutConstraint.activate([
            elementsScroll.topAnchor.constraint(equalTo: elementsContainer.topAnchor),
            elementsScroll.leadingAnchor.constraint(equalTo: elementsContainer.leadingAnchor),
            elementsScroll.trailingAnchor.constraint(equalTo: elementsContainer.trailingAnchor),
            elementsScroll.bottomAnchor.constraint(equalTo: elementsContainer.bottomAnchor),
        ])
    }

    // MARK: Tab switching

    @objc private func tabChanged() { showTab(segmented.selectedSegment) }
    @objc private func closeDevToolsClicked() { onCloseDevTools?() }

    private func showTab(_ index: Int) {
        consoleContainer.isHidden = index != 0
        netContainer.isHidden = index != 1
        elementsContainer.isHidden = index != 2
        // Leaving the Elements tab clears the on-page highlight.
        if index != 2 {
            if let engine = engineProvider?() {
                browser_engine_set_inspect_node(engine, -1)
            }
            onRefreshPage?()
        }
        refreshVisible()
    }

    var isConsoleTab: Bool { segmented.selectedSegment == 0 }
    var isElementsTab: Bool { segmented.selectedSegment == 2 }

    // MARK: REPL

    @objc private func replSubmit() {
        let input = replField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !input.isEmpty else { return }
        replLines.append("› " + input)
        if let engine = engineProvider?() {
            let result = input.withCString { browser_engine_console_eval(engine, $0) }
            if let result = result {
                replLines.append(String(cString: result))
            }
        }
        replField.stringValue = ""
        // An eval may have changed the DOM; ask the app to re-render (which also refreshes us).
        onRefreshPage?()
        refreshConsole()
        scrollConsoleToBottom()
        // Keep focus in the REPL for the next expression.
        window?.makeFirstResponder(replField)
    }

    /// Move focus into the REPL field (called when devtools is shown on the Console tab).
    func focusREPL() {
        if isConsoleTab { window?.makeFirstResponder(replField) }
    }

    // MARK: Refresh

    /// Clear the per-page REPL session (called on navigation / active-tab change).
    func clearREPL() {
        replLines.removeAll()
        refreshConsole()
    }

    /// Refresh whichever tab is currently visible. Cheap; called on the render/tick path.
    func refreshVisible() {
        guard !isHidden else { return }
        if isConsoleTab {
            refreshConsole()
        } else if isElementsTab {
            refreshElements()
        } else {
            refreshNetwork()
        }
    }

    /// Rebuild the DOM outline from the engine when the page changed, then refresh the row count
    /// header. Only runs while the Elements tab is visible (cheap-guard); rebuilds only when the DOM
    /// tree JSON differs from what's currently shown, so it's not re-parsed every tick.
    private func refreshElements(force: Bool = false) {
        guard !isHidden, isElementsTab else { return }
        var json = "{}"
        if let engine = engineProvider?(), let c = browser_engine_dom_tree(engine) {
            json = String(cString: c)
        }
        if force || json != domTreeJSON || domRoot == nil {
            domTreeJSON = json
            domRoot = DOMNode.parse(json)
            elementsOutline.reloadData()
            // Expand the top couple of levels so the tree isn't a single collapsed root.
            if let root = domRoot {
                elementsOutline.expandItem(root)
                for child in root.children { elementsOutline.expandItem(child) }
            }
        }
        let count = domRoot?.descendantCount ?? 0
        header.stringValue = "\(count) node\(count == 1 ? "" : "s")"
    }

    private func refreshConsole() {
        guard !isHidden, isConsoleTab else { return }
        var lines: [String] = []
        if let engine = engineProvider?(), let c = browser_engine_console_text(engine) {
            let text = String(cString: c)
            if !text.isEmpty { lines = text.components(separatedBy: "\n") }
        }
        lines.append(contentsOf: replLines)

        let attr = NSMutableAttributedString()
        let normal = NSColor(white: 0.85, alpha: 1.0)
        let errColor = NSColor(calibratedRed: 1.0, green: 0.45, blue: 0.45, alpha: 1.0)
        let replColor = NSColor(calibratedRed: 0.55, green: 0.85, blue: 1.0, alpha: 1.0)
        for (i, line) in lines.enumerated() {
            let lower = line.lowercased()
            let color: NSColor
            if line.hasPrefix("›") {
                color = replColor
            } else if lower.contains("uncaught") || lower.contains("error") {
                color = errColor
            } else {
                color = normal
            }
            let suffix = i == lines.count - 1 ? "" : "\n"
            attr.append(NSAttributedString(string: line + suffix, attributes: [
                .font: DevToolsView.mono,
                .foregroundColor: color,
            ]))
        }
        consoleText.textStorage?.setAttributedString(attr)
    }

    private func scrollConsoleToBottom() {
        consoleText.scrollToEndOfDocument(nil)
    }

    private func refreshNetwork() {
        guard !isHidden, !isConsoleTab else { return }
        var rows: [NetRow] = []
        if let engine = engineProvider?(), let c = browser_engine_network_log(engine) {
            let json = String(cString: c)
            if let data = json.data(using: .utf8),
               let arr = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] {
                for o in arr {
                    rows.append(NetRow(
                        method: o["method"] as? String ?? "",
                        url: o["url"] as? String ?? "",
                        status: (o["status"] as? NSNumber)?.intValue ?? 0,
                        ok: o["ok"] as? Bool ?? false,
                        ms: (o["ms"] as? NSNumber)?.doubleValue ?? 0,
                        size: (o["size"] as? NSNumber)?.intValue ?? 0,
                        type: o["type"] as? String ?? ""
                    ))
                }
            }
        }
        netRows = rows
        header.stringValue = "\(rows.count) request\(rows.count == 1 ? "" : "s")"
        netTable.reloadData()
    }

    // MARK: Elements selection / inspect

    /// Selection in the outline → highlight the node on the page. Clicking empty space clears it.
    @objc private func elementsRowClicked() {
        let row = elementsOutline.selectedRow
        guard let engine = engineProvider?() else { return }
        if row >= 0, let node = elementsOutline.item(atRow: row) as? DOMNode {
            browser_engine_set_inspect_node(engine, Int64(node.id))
        } else {
            browser_engine_set_inspect_node(engine, -1)
        }
        onRefreshPage?()
    }

    /// "Inspect Element" entry point: switch to the Elements tab, (re)build the tree if needed, then
    /// find, expand-to, select, scroll-to, and highlight the row for `nodeId`.
    func inspect(nodeId: Int) {
        segmented.selectedSegment = 2
        showTab(2)
        refreshElements(force: true)
        guard let root = domRoot, let target = root.find(id: nodeId) else { return }
        // Expand every ancestor so the target row is visible, then select + scroll to it.
        for ancestor in root.ancestors(of: target) {
            elementsOutline.expandItem(ancestor)
        }
        let row = elementsOutline.row(forItem: target)
        guard row >= 0 else { return }
        elementsOutline.selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
        elementsOutline.scrollRowToVisible(row)
        if let engine = engineProvider?() {
            browser_engine_set_inspect_node(engine, Int64(target.id))
        }
        onRefreshPage?()
    }
}

extension DevToolsView: NSTableViewDataSource, NSTableViewDelegate {
    func numberOfRows(in tableView: NSTableView) -> Int { netRows.count }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        guard let column = tableColumn, row < netRows.count else { return nil }
        let r = netRows[row]
        let id = column.identifier.rawValue
        let cell: NSTextField
        if let reused = tableView.makeView(withIdentifier: column.identifier, owner: self) as? NSTextField {
            cell = reused
        } else {
            cell = NSTextField(labelWithString: "")
            cell.identifier = column.identifier
            cell.font = DevToolsView.mono
            cell.lineBreakMode = .byTruncatingTail
            cell.drawsBackground = false
        }
        var color = NSColor(white: 0.85, alpha: 1.0)
        switch id {
        case "name": cell.stringValue = r.name
        case "method": cell.stringValue = r.method
        case "status":
            cell.stringValue = r.statusText
            if r.status == 0 || !(r.status >= 200 && r.status < 300) {
                color = NSColor(calibratedRed: 1.0, green: 0.45, blue: 0.45, alpha: 1.0)
            }
        case "type": cell.stringValue = r.typeShort
        case "size": cell.stringValue = r.sizeText
        case "time": cell.stringValue = r.timeText
        default: cell.stringValue = ""
        }
        cell.textColor = color
        cell.toolTip = r.url
        return cell
    }
}

// MARK: - DOM tree model (Elements tab)

/// A lightweight, reference-typed node parsed from the engine's `browser_engine_dom_tree` JSON.
/// Reference type so `NSOutlineView` can use the instances as opaque items (identity-based).
final class DOMNode {
    let id: Int
    let isElement: Bool
    let tag: String
    let attrs: [(String, String)]
    let text: String
    let children: [DOMNode]
    weak var parent: DOMNode?

    /// True for the synthetic `</tag>` row shown after an expanded element's children.
    var isClosing = false
    private var _closingNode: DOMNode?
    /// A cached `</tag>` pseudo-node (same id, so selecting it highlights the same element).
    var closingNode: DOMNode {
        if let n = _closingNode { return n }
        let n = DOMNode(id: id, isElement: true, tag: tag, attrs: [], text: "", children: [])
        n.isClosing = true
        n.parent = self
        _closingNode = n
        return n
    }

    init(id: Int, isElement: Bool, tag: String, attrs: [(String, String)], text: String, children: [DOMNode]) {
        self.id = id
        self.isElement = isElement
        self.tag = tag
        self.attrs = attrs
        self.text = text
        self.children = children
        for c in children { c.parent = self }
    }

    /// Parse the engine's DOM tree JSON (a single nested object) into a `DOMNode`, or nil on
    /// `"{}"` / malformed input.
    static func parse(_ json: String) -> DOMNode? {
        guard let data = json.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }
        return build(obj)
    }

    private static func build(_ o: [String: Any]) -> DOMNode? {
        let id = (o["id"] as? NSNumber)?.intValue ?? -1
        let type = o["type"] as? String ?? ""
        if type == "text" {
            return DOMNode(id: id, isElement: false, tag: "", attrs: [],
                           text: o["text"] as? String ?? "", children: [])
        }
        guard type == "element" else { return nil }
        var attrs: [(String, String)] = []
        if let a = o["attrs"] as? [String: Any] {
            // Preferred order: id, class, then the rest alphabetically.
            let ordered = a.keys.sorted { lhs, rhs in
                func rank(_ k: String) -> Int { k == "id" ? 0 : (k == "class" ? 1 : 2) }
                let (rl, rr) = (rank(lhs), rank(rhs))
                return rl != rr ? rl < rr : lhs < rhs
            }
            for k in ordered { attrs.append((k, (a[k] as? String) ?? "")) }
        }
        var kids: [DOMNode] = []
        if let cs = o["children"] as? [[String: Any]] {
            for c in cs { if let n = build(c) { kids.append(n) } }
        }
        return DOMNode(id: id, isElement: true, tag: o["tag"] as? String ?? "", attrs: attrs,
                       text: "", children: kids)
    }

    /// Total node count of this subtree (including self), for the header label.
    var descendantCount: Int {
        1 + children.reduce(0) { $0 + $1.descendantCount }
    }

    /// Depth-first search for a node by its engine NodeId.
    func find(id target: Int) -> DOMNode? {
        if id == target { return self }
        for c in children { if let f = c.find(id: target) { return f } }
        return nil
    }

    /// The chain of ancestors of `node` (root-first, excluding `node` itself).
    func ancestors(of node: DOMNode) -> [DOMNode] {
        var chain: [DOMNode] = []
        var cur = node.parent
        while let c = cur { chain.append(c); cur = c.parent }
        return chain.reversed()
    }
}

extension DevToolsView: NSOutlineViewDataSource, NSOutlineViewDelegate {
    func outlineView(_ outlineView: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
        guard let node = item as? DOMNode else { return domRoot == nil ? 0 : 1 }
        if node.isClosing || node.children.isEmpty { return 0 }
        return node.children.count + 1 // +1 for the synthetic </tag> row
    }

    func outlineView(_ outlineView: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
        guard let node = item as? DOMNode else { return domRoot! }
        return index < node.children.count ? node.children[index] : node.closingNode
    }

    func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
        guard let node = item as? DOMNode, !node.isClosing else { return false }
        return !node.children.isEmpty
    }

    func outlineView(_ outlineView: NSOutlineView, viewFor tableColumn: NSTableColumn?, item: Any) -> NSView? {
        guard let node = item as? DOMNode else { return nil }
        let id = NSUserInterfaceItemIdentifier("domCell")
        let cell: NSTextField
        if let reused = outlineView.makeView(withIdentifier: id, owner: self) as? NSTextField {
            cell = reused
        } else {
            cell = NSTextField(labelWithString: "")
            cell.identifier = id
            cell.font = DevToolsView.mono
            cell.lineBreakMode = .byTruncatingTail
            cell.drawsBackground = false
        }
        if node.isClosing {
            cell.attributedStringValue = DevToolsView.closingRow(node)
        } else if node.isElement {
            cell.attributedStringValue = DevToolsView.elementRow(node)
        } else {
            // Text node: quoted, truncated, muted.
            let t = DevToolsView.truncate(node.text, 80)
            cell.stringValue = "\"\(t)\""
            cell.textColor = NSColor(white: 0.55, alpha: 1.0)
        }
        return cell
    }

    func outlineViewSelectionDidChange(_ notification: Notification) {
        // Selection can change via keyboard too; mirror the click handler.
        let row = elementsOutline.selectedRow
        guard let engine = engineProvider?() else { return }
        if row >= 0, let node = elementsOutline.item(atRow: row) as? DOMNode {
            browser_engine_set_inspect_node(engine, Int64(node.id))
        } else {
            browser_engine_set_inspect_node(engine, -1)
        }
        onRefreshPage?()
    }

    // MARK: Row rendering

    /// An HTML-ish opening tag for an element row, e.g. `<div id="main" class="box">`, with the tag
    /// name and attribute names/values colored.
    static func elementRow(_ node: DOMNode) -> NSAttributedString {
        let tagColor = NSColor(calibratedRed: 0.45, green: 0.75, blue: 1.0, alpha: 1.0)
        let attrNameColor = NSColor(calibratedRed: 0.85, green: 0.65, blue: 0.45, alpha: 1.0)
        let attrValColor = NSColor(calibratedRed: 0.55, green: 0.80, blue: 0.55, alpha: 1.0)
        let punct = NSColor(white: 0.55, alpha: 1.0)
        let font = DevToolsView.mono

        let s = NSMutableAttributedString()
        func add(_ str: String, _ color: NSColor) {
            s.append(NSAttributedString(string: str, attributes: [.font: font, .foregroundColor: color]))
        }
        add("<", punct)
        add(node.tag, tagColor)
        // Show up to a few attributes (already ordered id, class, then the rest); truncate values.
        for (k, v) in node.attrs.prefix(6) {
            add(" ", punct)
            add(k, attrNameColor)
            add("=\"", punct)
            add(truncate(v, 40), attrValColor)
            add("\"", punct)
        }
        if node.attrs.count > 6 { add(" …", punct) }
        add(">", punct)
        return s
    }

    /// A `</tag>` closing-tag row (shown after an expanded element's children).
    static func closingRow(_ node: DOMNode) -> NSAttributedString {
        let tagColor = NSColor(calibratedRed: 0.45, green: 0.75, blue: 1.0, alpha: 1.0)
        let punct = NSColor(white: 0.55, alpha: 1.0)
        let font = DevToolsView.mono
        let s = NSMutableAttributedString()
        func add(_ str: String, _ color: NSColor) {
            s.append(NSAttributedString(string: str, attributes: [.font: font, .foregroundColor: color]))
        }
        add("</", punct); add(node.tag, tagColor); add(">", punct)
        return s
    }

    static func truncate(_ s: String, _ max: Int) -> String {
        let collapsed = s.replacingOccurrences(of: "\n", with: " ")
        if collapsed.count <= max { return collapsed }
        return String(collapsed.prefix(max)) + "…"
    }
}

