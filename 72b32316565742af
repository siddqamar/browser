let iframe;

/**
 * Test whether JS syntax is supported.
 * @param {*} syntax
 * @returns
 */
export default function syntax (syntax) {
	let result = syntaxSync(syntax);

	if (result.success) {
		return result;
	}

	// Many things don't work inside functions (e.g. imports), try again
	return syntaxAsync(syntax);
}

export function syntaxSync (syntax) {
	try {
		new Function(syntax);
		return {success: true};
	}
	catch (error) {}

	return {success: false};
}

export async function syntaxAsync (syntax, options = {}) {
	try {
		await import(`data:text/javascript,${syntax}`);
		return {success: true};
	}
	catch (error) {}

	// As a last resort, try an iframe
	let base64Syntax = btoa(syntax);
	iframe ??= document.createElement('iframe');
	iframe.style = 'display: none';
	iframe.setAttribute('sandbox', 'allow-scripts');
	iframe.setAttribute('srcdoc', `
		<script>
			let success = true;
			try { eval(atob('${base64Syntax}')) }
			catch (e) {
				success = false;
			}
			parent.postMessage({syntax: '${ base64Syntax }', success: success}, '*')
		</script>
	`);

	document.body.appendChild(iframe);

	return new Promise(resolve => {
		addEventListener('message', event => {
			if (event.data.syntax === base64Syntax) {
				iframe.remove();
				resolve({success: event.data.success});
			}
		}, {once: true});

		setTimeout(() => {
			iframe.remove();
			resolve({success: false});
		}, options.timeout ?? 100);
	});
}
