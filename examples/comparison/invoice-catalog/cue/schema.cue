package invoicecomparison

import (
	"list"
	"strings"
)

#Text1To20: string & strings.MinRunes(1) & strings.MaxRunes(20)
#Text1To30: string & strings.MinRunes(1) & strings.MaxRunes(30)
#Text1To60: string & strings.MinRunes(1) & strings.MaxRunes(60)
#Text1To80: string & strings.MinRunes(1) & strings.MaxRunes(80)

#Address: {
	city!:    #Text1To60
	country!: "mx" | "us" | "ca"
}

#Customer: {
	name!:    #Text1To80
	address!: #Address
}

#Line: {
	sku!:              #Text1To20
	kind!:             "service" | "goods"
	quantity!:         int & >=1 & <=99
	unit_price_cents!: int & >=1 & <=100000
	total_cents:       quantity * unit_price_cents
}

#Invoice: {
	id!:       #Text1To30
	number!:   #Text1To30
	status!:   "draft" | "issued" | "paid"
	currency!: "mxn" | "usd" | "cad"
	customer!: #Customer
	lines!: [...#Line] & list.MinItems(1) & list.MaxItems(8)
	total_cents: list.Sum([for line in lines {
		line.total_cents
	}])
	// Invoices above this computed amount require a different approval path.
	total_cents: <=500000
}

invoice: #Invoice
