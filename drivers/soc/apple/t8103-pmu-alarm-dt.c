// SPDX-License-Identifier: GPL-2.0-only OR MIT
/*
 * Apple M1 (t8103): describe the Sera PMU's RTC alarm in the boot loader's
 * device tree.
 *
 * rtc-macsmc uses the PMU alarm as a wake source when the device tree gives
 * the SMC RTC the alarm's NVMEM cells and interrupt, and spmi-apple delivers
 * PMU interrupts when its node has an interrupt of its own. The t8103 trees
 * m1n1 passes have neither, and a lab kernel boots with m1n1's tree, not its
 * own. So this adds them at arch_initcall, before any device is created from
 * the tree, and every driver probes as if the tree had always had them:
 *
 *  - nub-spmi (0x23d0d9300) becomes an interrupt controller on AIC 343, its
 *    only AIC line in the ADT (/arm-io/nub-spmi "interrupts", 2nd entry);
 *  - the Sera PMU (pmic@f) layout gains the alarm cells, at the offsets the
 *    ADT gives (/arm-io/nub-spmi/spmi-pmu info-rtc_alarm_ctrl 0xd000,
 *    info-rtc_alarm_offset 0xd008, info-rtc_alarm_event 0xd00c,
 *    info-rtc_irq_mask_offset 0xd00e);
 *  - the SMC RTC gets those cells, the PMU's SPMI interrupt 2 (spmi-pmu
 *    "interrupts") and wakeup-source.
 *
 * This is opt-in while the alarm is unproven on t8103 hardware: nothing is
 * done unless the kernel is booted with apple_t8103_pmu_alarm=1. Even then,
 * nothing is changed unless all of the nodes are found as expected, nor when
 * the tree already describes the alarm.
 *
 * Copyright (C) The Asahi Linux Contributors
 */

#define pr_fmt(fmt) "t8103-pmu-alarm: " fmt

#include <linux/init.h>
#include <linux/irq.h>
#include <linux/kernel.h>
#include <linux/minmax.h>
#include <linux/moduleparam.h>
#include <linux/of.h>
#include <linux/of_address.h>

#define NUB_SPMI_BASE		0x23d0d9300ULL
#define NUB_SPMI_AIC_IRQ	343
#define SERA_SPMI_IRQ		2
/* First cell of an AIC specifier: AIC_IRQ (dt-bindings apple-aic.h) */
#define DT_AIC_IRQ		0

static bool enable;
core_param(apple_t8103_pmu_alarm, enable, bool, 0444);

struct alarm_cell {
	const char *con_id;	/* rtc-macsmc's nvmem-cell-names entry */
	const char *node;
	u32 reg;
	u32 len;
};

static const struct alarm_cell cells[] __initconst = {
	{ "rtc_ctrl",		"rtc-ctrl@d000",	0xd000, 1 },
	{ "rtc_alarm",		"rtc-alarm@d008",	0xd008, 4 },
	{ "rtc_alarm_mask",	"rtc-alarm-mask@d00e",	0xd00e, 1 },
	{ "rtc_alarm_event",	"rtc-alarm-event@d00c",	0xd00c, 1 },
};
#define NCELLS ARRAY_SIZE(cells)

static struct of_changeset alarm_cs;

static phandle __init max_phandle(void)
{
	struct device_node *np;
	phandle max = 0;

	for (np = of_find_all_nodes(NULL); np; np = of_find_all_nodes(np))
		max = max(max, np->phandle);
	return max;
}

/* The layout's cell at offset @reg; takes a reference. */
static struct device_node * __init find_cell(struct device_node *layout, u32 reg,
					     u32 *len)
{
	struct device_node *child;
	u32 r[2];

	for_each_child_of_node(layout, child) {
		if (!of_property_read_u32_array(child, "reg", r, 2) && r[0] == reg) {
			*len = r[1];
			return child;
		}
	}
	return NULL;
}

/*
 * of_attach_node() reads a node's phandle from its "phandle" property, which
 * a changeset only adds after the attach, and adding the property to an
 * attached node leaves np->phandle alone. So the phandles are allocated here
 * above every one in the tree, added as properties for anyone reading the
 * tree, and set on the nodes once the changeset is applied.
 */
static int __init give_phandle(struct device_node *np, phandle *next, phandle *ph,
			       bool *set)
{
	if (np->phandle) {
		*ph = np->phandle;
		*set = false;
		return 0;
	}
	*ph = ++*next;
	*set = true;
	return of_changeset_add_prop_u32(&alarm_cs, np, "phandle", *ph);
}

static int __init t8103_pmu_alarm_dt(void)
{
	struct device_node *pmic = NULL, *spmi = NULL, *layout = NULL;
	struct device_node *rtc = NULL, *aic = NULL, *off = NULL;
	struct device_node *cell_np[NCELLS] = {};
	bool found[NCELLS] = {}, set_ph[NCELLS] = {}, set_spmi_ph = false;
	const char *names[NCELLS + 1];
	u32 phs[NCELLS + 1];
	phandle next, spmi_ph = 0;
	struct resource res;
	bool add_spmi_irq;
	u32 icells, len;
	int i, idx, ret;

	if (!of_machine_is_compatible("apple,t8103"))
		return 0;
	if (!enable) {
		pr_info("off: boot with apple_t8103_pmu_alarm=1 to describe the PMU alarm\n");
		return 0;
	}

	pmic = of_find_compatible_node(NULL, NULL, "apple,sera-pmic");
	if (pmic) {
		spmi = of_get_parent(pmic);
		layout = of_get_child_by_name(pmic, "nvmem-layout");
	}
	rtc = of_find_compatible_node(NULL, NULL, "apple,smc-rtc");
	aic = of_find_compatible_node(NULL, NULL, "apple,aic");
	if (!spmi || !layout || !rtc || !aic) {
		pr_info("no Sera PMU, SPMI controller, SMC RTC or AIC node: nothing done\n");
		goto out;
	}
	if (!of_device_is_compatible(spmi, "apple,t8103-spmi") ||
	    of_address_to_resource(spmi, 0, &res) || res.start != NUB_SPMI_BASE) {
		pr_warn("%pOF is not the t8103 nub-spmi at %#llx: nothing done\n",
			spmi, NUB_SPMI_BASE);
		goto out;
	}
	if (of_property_match_string(rtc, "nvmem-cell-names", "rtc_ctrl") >= 0) {
		pr_info("%pOF already describes the alarm: nothing done\n", rtc);
		goto out;
	}
	if (of_property_count_strings(rtc, "nvmem-cell-names") != 1 ||
	    of_property_present(rtc, "interrupts") ||
	    of_property_present(rtc, "interrupts-extended")) {
		pr_warn("%pOF is not the expected offset-only RTC: nothing done\n", rtc);
		goto out;
	}
	idx = of_property_match_string(rtc, "nvmem-cell-names", "rtc_offset");
	off = idx < 0 ? NULL : of_parse_phandle(rtc, "nvmem-cells", idx);
	if (!off || !off->phandle || off->parent != layout) {
		pr_warn("%pOF has no rtc_offset cell in %pOF: nothing done\n", rtc, layout);
		goto out;
	}
	if (!aic->phandle || of_property_read_u32(aic, "#interrupt-cells", &icells) ||
	    icells != 3) {
		pr_warn("%pOF is not a 3-cell AIC with a phandle: nothing done\n", aic);
		goto out;
	}
	add_spmi_irq = !of_property_present(spmi, "interrupts") &&
		       !of_property_present(spmi, "interrupts-extended");
	if (!add_spmi_irq && !of_property_read_bool(spmi, "interrupt-controller")) {
		pr_warn("%pOF has an interrupt but is no interrupt controller: nothing done\n",
			spmi);
		goto out;
	}

	next = max_phandle();
	of_changeset_init(&alarm_cs);

	ret = give_phandle(spmi, &next, &spmi_ph, &set_spmi_ph);
	if (!ret && add_spmi_irq) {
		const u32 irq[3] = { DT_AIC_IRQ, NUB_SPMI_AIC_IRQ, IRQ_TYPE_LEVEL_HIGH };

		if (of_property_present(spmi, "interrupt-parent"))
			ret = of_changeset_remove_property(&alarm_cs, spmi,
							   of_find_property(spmi, "interrupt-parent", NULL));
		if (!ret)
			ret = of_changeset_add_prop_u32(&alarm_cs, spmi, "interrupt-parent",
							aic->phandle);
		if (!ret)
			ret = of_changeset_add_prop_u32_array(&alarm_cs, spmi, "interrupts",
							      irq, ARRAY_SIZE(irq));
		if (!ret && !of_property_present(spmi, "interrupt-controller"))
			ret = of_changeset_add_prop_bool(&alarm_cs, spmi,
							 "interrupt-controller");
		if (!ret && !of_property_present(spmi, "#interrupt-cells"))
			ret = of_changeset_add_prop_u32(&alarm_cs, spmi, "#interrupt-cells", 2);
	}

	names[0] = "rtc_offset";
	phs[0] = off->phandle;
	for (i = 0; i < NCELLS && !ret; i++) {
		cell_np[i] = find_cell(layout, cells[i].reg, &len);
		if (cell_np[i]) {
			found[i] = true;
			if (len != cells[i].len) {
				pr_warn("%pOF is %u bytes, not %u: nothing done\n",
					cell_np[i], len, cells[i].len);
				ret = -EINVAL;
				break;
			}
		} else {
			/* The changeset keeps the node; the reference returned is ours to keep. */
			cell_np[i] = of_changeset_create_node(&alarm_cs, layout, cells[i].node);
			if (!cell_np[i]) {
				ret = -ENOMEM;
				break;
			}
			ret = of_changeset_add_prop_u32_array(&alarm_cs, cell_np[i], "reg",
							      (const u32[]){ cells[i].reg,
									     cells[i].len }, 2);
		}
		if (!ret)
			ret = give_phandle(cell_np[i], &next, &phs[i + 1], &set_ph[i]);
		names[i + 1] = cells[i].con_id;
	}

	if (!ret)
		ret = of_changeset_remove_property(&alarm_cs, rtc,
						   of_find_property(rtc, "nvmem-cells", NULL));
	if (!ret)
		ret = of_changeset_remove_property(&alarm_cs, rtc,
						   of_find_property(rtc, "nvmem-cell-names", NULL));
	if (!ret)
		ret = of_changeset_add_prop_u32_array(&alarm_cs, rtc, "nvmem-cells",
						      phs, ARRAY_SIZE(phs));
	if (!ret)
		ret = of_changeset_add_prop_string_array(&alarm_cs, rtc, "nvmem-cell-names",
							 names, ARRAY_SIZE(names));
	if (!ret)
		ret = of_changeset_add_prop_u32_array(&alarm_cs, rtc, "interrupts-extended",
						      (const u32[]){ spmi_ph, SERA_SPMI_IRQ,
								     IRQ_TYPE_EDGE_RISING }, 3);
	if (!ret && !of_property_present(rtc, "wakeup-source"))
		ret = of_changeset_add_prop_bool(&alarm_cs, rtc, "wakeup-source");

	if (!ret)
		ret = of_changeset_apply(&alarm_cs);
	if (ret) {
		of_changeset_destroy(&alarm_cs);
		/* The nodes this created were never attached: drop them too. */
		for (i = 0; i < NCELLS; i++)
			if (cell_np[i] && !found[i])
				of_node_put(cell_np[i]);
		pr_err("could not describe the PMU alarm: %d\n", ret);
		goto out_cells;
	}

	/* Single-threaded at arch_initcall: nothing can look a phandle up meanwhile. */
	if (set_spmi_ph)
		spmi->phandle = spmi_ph;
	for (i = 0; i < NCELLS; i++)
		if (set_ph[i])
			cell_np[i]->phandle = phs[i + 1];

	pr_info("%pOF: interrupt %s; %pOF: alarm cells 0xd000/0xd008/0xd00c/0xd00e and SPMI interrupt %d, wakeup-source\n",
		spmi, add_spmi_irq ? "AIC 343 added" : "kept", rtc, SERA_SPMI_IRQ);

out_cells:
	for (i = 0; i < NCELLS; i++)
		if (found[i])
			of_node_put(cell_np[i]);
out:
	of_node_put(off);
	of_node_put(aic);
	of_node_put(rtc);
	of_node_put(layout);
	of_node_put(spmi);
	of_node_put(pmic);
	return 0;
}
/* Runs before of_platform_default_populate_init() at arch_initcall_sync. */
arch_initcall(t8103_pmu_alarm_dt);
