import supportsRule, { prefixRule } from './rule.js';
import { prefixes } from '../../data.js';

let cached = {};

const CAN_USE_CSS_SUPPORTS = 'CSS' in globalThis && CSS.supports && CSS.supports('selector(p)');

export default function selector (selector) {
	if (!CAN_USE_CSS_SUPPORTS) {
		return supportsRule(selector);
	}

	let cachedResult = cached[selector];
	let success, prefix;

	if (cachedResult !== undefined) {
		success = Boolean(cachedResult);
		prefix = typeof cachedResult === "boolean" ? '' : cachedResult;

		return { success, prefix };
	}

	for (let prefix of prefixes) {
		let resolved = prefixRule(selector, prefix);

		if (CSS.supports('selector(' + resolved + ')')) {
			cached[selector] = prefix === '' ? true : (prefix ?? false);
			return {
				success: true,
				prefix,
			};
		}
	}

	cached[selector] = false;
	return { success: false };
}
