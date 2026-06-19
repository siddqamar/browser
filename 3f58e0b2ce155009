import { camelCase } from '../../util.js';

let elementInterfaces = {};

/**
 * Irregular cases that can't be determined by `camelCase()`
 */
export const irregularAttributes = {
	contenteditable: 'contentEditable',
	readonly: 'readOnly',
	for: 'htmlFor',
	class: 'className',
	tabindex: 'tabIndex',
	maxlength: 'maxLength',
	minlength: 'minLength',
	colspan: 'colSpan',
	rowspan: 'rowSpan',
	usemap: 'useMap',
	ismap: 'isMap',
	datetime: 'dateTime',
	autocapitalize: 'autoCapitalize',
	autofocus: 'autoFocus',
	autoplay: 'autoPlay',
	playsinline: 'playsInline',
	crossorigin: 'crossOrigin',
};

export function getElementInterface (elementType) {
	if (!elementType || elementType.includes('-')) {
		return HTMLElement;
	}

	if (!elementInterfaces[elementType]) {
		let Class;
		try {
			// TODO support other namespaces
			Class = document.createElement(elementType).constructor;
		}
		catch (error) {
			Class = HTMLElement;
		}

		if (Class === HTMLUnknownElement) {
			Class = HTMLElement;
		}

		elementInterfaces[elementType] = Class;
	}

	return elementInterfaces[elementType];
}

export function getAttributeJsName (attributeName) {
	return irregularAttributes[attributeName] ?? camelCase(attributeName);
}

export default function attribute (
	name,
	{
		jsName = getAttributeJsName(name),
		elementType = '_',
		elementInterface = getElementInterface(elementType),
	} = {},
) {
	try {
		let success = jsName in elementInterface.prototype;
		return { success, jsName, elementInterface };
	}
	catch (error) {
		// Unknown properties don't throw errors
		return { success: true, jsName, elementInterface };
	}
}
