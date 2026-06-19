import isAttributeSupported, { getElementInterface, getAttributeJsName } from './attribute.js';

let dummies = {};
let cached = {};

export default function attributeValue (
	name,
	value,
	{ elementType = '_', elementInterface = getElementInterface(elementType) } = {},
) {
	cached[elementType] ??= {};
	cached[elementType][name] ??= {};

	let cachedResult = cached[elementType][name][value];
	if (cachedResult) {
		return cachedResult;
	}

	let attributeSupported = isAttributeSupported(name, { elementType, elementInterface });
	let jsName = attributeSupported.jsName;
	elementInterface = attributeSupported.elementInterface;
	let element = (dummies[elementType] ??= document.createElement(elementType));

	let defaultValue = element[jsName];
	let isBoolean = typeof defaultValue === 'boolean';

	element[jsName] = value;
	let result = element[jsName];
	element[jsName] = defaultValue;

	let success;
	if (isBoolean && (value === '' || name === value.toLowerCase())) {
		// Support cases like hidden="", hidden="hidden" (case insensitive), etc.
		success = true;
	}
	else if (isBoolean && result === true && typeof value !== 'boolean') {
		// Boolean attribute expanded to take values, but doesn't support the value
		// Example: hidden="until-found" not supported, but hidden is
		success = false;
	}
	else if (result === defaultValue && value !== defaultValue) {
		// Value was rejected
		success = false;
	}
	else {
		success = true;
	}

	return (cached[elementType][name][value] = { success, attribute: attributeSupported });
}
