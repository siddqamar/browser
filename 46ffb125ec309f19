import { styleElement } from '../../shared.js';
import { prefixes } from '../../data.js';

let cached = {};
let instances = {};

/**
 * Low-level function to check if a rule is supported. No cache, no input fixup, no prefixing.
 * @param {string} name - The rule to check as a string (e.g. "@supports (display: flex)").
 * @param {CSSRule | CSSStyleSheet} [parentRule] - Optionally a parent rule to insert the rule into.
 * @returns {boolean}
 */
export function isSupported (rule, { parent, contentBefore = '' } = {}) {
	let code = rule;

	if (!rule.endsWith(';') && !rule.endsWith('}')) {
		code += `{ }`;
	}

	if (contentBefore) {
		code = contentBefore + code;
	}

	if (parent) {
		// Rules that are only valid inside other rules, e.g. @stylistic
		let parentRule = parent.instance;

		if (parentRule && parentRule.cssRules && (parentRule.insertRule || parentRule.appendRule)) {
			if (parentRule.insertRule) {
				// Most rules
				parentRule.insertRule(code, 0);
			}
			else if (parentRule.appendRule) {
				// E.g. CSSKeyframeRule
				parentRule.appendRule(code);
			}

			return parentRule.cssRules[0];
		}
		else {
			// Not an object we can use, fall back to using code :(
			code = `${parent.resolved} { ${code} }`;
		}
	}

	if (parent || contentBefore) {
		let codeWithout = parent ? `${parent.resolved} { ${contentBefore} }` : contentBefore;

		styleElement.textContent = codeWithout;

		let cssTextWithout = styleElement.sheet.cssRules[0].cssText;

		styleElement.textContent = code;
		let cssText = styleElement.sheet.cssRules[0].cssText;

		return cssText !== cssTextWithout;
	}
	else {
		styleElement.textContent = code;
		return styleElement.sheet.cssRules[0];
	}

}

/**
 * Cached.
 * @param {string} rule An @-rule (including any prelude like e.g. "@supports (display: flex)"), a selector, a pseudo-element etc.
 * @returns
 */
export default function supportsRule (rule, {parent: parentRule, contentBefore} = {}) {
	let parent;

	if (parentRule) {
		// Fail early if parent not supported
		parent = supportsRule(parentRule);
		if (!parent.success) {
			return {success: false, parent: parent};
		}
	}

	rule = rule.trim();

	let cachedResult = cached[rule];
	let success, prefix, resolved;

	if (cachedResult === undefined) {
		for (let p of prefixes) {
			resolved = prefixRule(rule, p);
			success = isSupported(resolved, {parent, contentBefore});

			if (success) {
				if (typeof success === 'object') {
					instances[rule] = success;
				}

				prefix = p;
				success = true;
				break;
			}
		}

		cached[rule] = prefix === '' ? true : (prefix ?? false);
	}
	else {
		success = Boolean(cachedResult);
		prefix = typeof cachedResult === "boolean" ? '' : cachedResult;
		resolved = prefixRule(rule, prefix);
	}

	success ??= false;

	return {success, prefix, instance: instances[rule], resolved: success ? resolved : rule};
}

export function prefixRule (rule, prefix = '') {
	return rule.replace(/^[@:]*/, '$&' + prefix);
}
