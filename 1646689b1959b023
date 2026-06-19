import supportsElement from '../html/element.js';

/**
 * Check whether and event is supported.
 * If not trigger function is provided, this checks for the presence of the on[event-name] property on the event target.
 * Important to note that not all events can be detected this way (and for many there is no suitable trigger, e.g. DOMContentLoaded).
 * @param {string} name - The name of the event to check.
 * @param {object} [options] - The options for the event.
 * @param {string | Object} [options.on] - The event target to check (defaults to HTMLElement.prototype).
 * @param {string} [options.element] - The element tag name to check. Ignored if on is provided.
 * @param {Function} [options.trigger] - A function that will trigger the event on the event target, if it is supported. If async, the result will be a promise.
 * @returns {object | Promise<object>}
 */
export default function event (name, options) {
	let eventTarget = HTMLElement.prototype;

	if (options.on) {
		eventTarget = typeof options.on === 'string' ? window[options.on]?.prototype : options.on;
	}
	else if (options.element) {
		let elementSupported = supportsElement(options.element);

		if (!elementSupported.success) {
			return {success: false, note: `Element "${options.element}" not supported`};
		}

		eventTarget = window[elementSupported.interface];
	}

	if (!eventTarget) {
		return {success: false, note: 'No event target to check'};
	}

	if (options.trigger) {
		let success = false;
		let fn = () => {
			success = true;
		};
		eventTarget.addEventListener(name, fn);
		let done = options.trigger.call(eventTarget);

		if (done instanceof Promise) {
			// Async trigger
			return done.then(() => {
				eventTarget.removeEventListener(name, fn);
				return {success, eventTarget};
			});
		}

		eventTarget.removeEventListener(name, fn);
		return {success, eventTarget};
	}

	if ('on' + name in eventTarget) {
		return {success: true, eventTarget};
	}

	return { success: false };
}
