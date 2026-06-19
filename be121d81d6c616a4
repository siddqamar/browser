import supportsValue from './value.js';

/**
 * Metadata about each CSS data type
 */
export const dataTypes = {
	length: { property: 'width', sampleValue: '0px' },
	percentage: { property: 'width', sampleValue: '0%' },

	time: { property: 'transition-duration', sampleValue: '0s' },
	angle: { property: { name: 'transform', value: v => `rotate(${v})` }, sampleValue: '0deg' },
	integer: { property: 'z-index', sampleValue: '0' },
	number: { property: 'line-height', sampleValue: '0' },
	frequency: { sampleValue: '0Hz' },
	resolution: { property: 'image-resolution', sampleValue: '0dpi' },
	image: {
		property: 'background-image',
		sampleValue:
			'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8zwAAAgMBgYUlKQAAAABJRU',
	},
	string: { property: 'content', sampleValue: '"test"' },
	'custom-ident': { property: 'animation-name', sampleValue: 'foo' },
	'dashed-ident': { sampleValue: '--foo' },
	color: { property: 'color', sampleValue: 'red' },

	// https://drafts.csswg.org/css-values/#mixed-percentages
	'length-percentage': { property: 'width', sampleValue: 'calc(1px + 1%)' },
	'frequency-percentage': { sampleValue: 'calc(1Hz + 1%)' },
	'angle-percentage': { sampleValue: 'calc(1deg + 1%)' },
	'time-percentage': { sampleValue: 'calc(1s + 1%)' },
};

dataTypes.url = dataTypes.image;


/**
 * Test whether a certain value is accepted as part of a given type
 * @param {string} value
 * @param {string} type
 * @returns
 */
export default function  (value, type) {
	let property = dataTypes[type]?.property;

	if (!property) {
		return { success: undefined, note: `Unknown type: ${type}` };
	}

	if (typeof property === 'object') {
		value = property.value(value);
		property = property.name;
	}

	return supportsValue(property, value);
}
