export function camelCase (str) {
	return str
		.replace(/-([a-z])/g, function ($0, $1) {
			return $1.toUpperCase();
		})
		.replace('-', '');
}

export function toArray (value) {
	if (value === undefined || value === null) {
		return [];
	}

	return Array.isArray(value) ? value : [value];
}

export function prefixCamelCase (prefix, name) {
	if (!prefix) {
		return name;
	}

	let capitalizedName = name.charAt(0).toUpperCase() + name.slice(1);
	return prefix + capitalizedName;
}
