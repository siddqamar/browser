import supportsRule from './rule.js';
export { isSupported } from './rule.js';

/**
 * Cached.
 * @param {string} rule An @-rule string including any prelude like e.g. "@supports (display: flex)", with or without the @
 * @returns
 */
export default function supportsAtRule (atrule, options) {
	if (!atrule.startsWith('@')) {
		atrule = "@" + atrule;
	}

	return supportsRule(atrule, options);
}
