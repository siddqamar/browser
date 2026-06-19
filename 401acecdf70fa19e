let cached = {};

export default function element (name) {
	let cachedResult = cached[name];

	if (cachedResult !== undefined) {
		let success = Boolean(cachedResult);
		let interfaceName = success ? cachedResult : 'HTMLUnknownElement';

		return {
			success,
			interface: interfaceName,
		};
	}

	let element = document.createElement(name);
	let interfaceName = element.constructor.name;

	let success = interfaceName !== 'HTMLUnknownElement';
	cached[name] = success ? interfaceName : false;

	cached[name] = {
		success,
		interface: interfaceName,
	};

	return cached[name];
}
