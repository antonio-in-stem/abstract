package invoicecomparison

invoice: {
	id:       "inv_1001"
	number:   "INV-1001"
	status:   "issued"
	currency: "mxn"
	customer: {
		name: "Acme Studio"
		address: {city: "Mexico City", country: "mx"}
	}
	lines: [{sku: "CONSULT", kind: "service", quantity: 0, unit_price_cents: 15000}]
}
