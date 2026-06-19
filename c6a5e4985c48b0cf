import testType from './type.js';

export default function  (unit, type) {
	if (globalThis.CSS && CSS.px) {
		// We can rely on typed OM
		let success = unit in CSS && typeof CSS[unit] === 'function';
		return { success };
	}

	return testType('1' + unit, type);
}
