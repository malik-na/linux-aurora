// SPDX-License-Identifier: GPL-2.0-only OR MIT
/*
 * Apple M1 (t8103) PMP v1 lab experiment: built-in device tree and power
 * domain support for the apple-pmp-v1 module.
 *
 * The device tree comes from the boot loader, so the PMP nodes cannot be
 * relied on to be there. When the module loads it calls
 * apple_pmp_v1_dt_setup(), which
 *  - keeps ps_pmp and ps_pms_sram powered from then on, through runtime PM
 *    and system sleep: pms_sram holds the only copy of the PMP firmware;
 *  - applies the built-in overlay (pmp-v1.dtso) that adds the PMP, its DART
 *    and its mailbox, gives them the base tree's interrupt parent and ps_pmp
 *    power domain, and enables them, which creates their platform devices.
 *
 * Nothing above can bring back firmware that genpd already powered off as
 * unused at late_initcall_sync. Either boot with pd_ignore_unused, or with
 * apple_pmp_v1_hold=1, which marks ps_pmp and ps_pms_sram apple,always-on
 * at arch_initcall, before the PMGR driver probes, so only those two
 * domains stay on. Without either, this file does nothing at boot.
 *
 * Copyright (C) The Asahi Linux Contributors
 */

#define pr_fmt(fmt) "apple-pmp-v1: " fmt

#include <linux/device.h>
#include <linux/export.h>
#include <linux/init.h>
#include <linux/moduleparam.h>
#include <linux/mutex.h>
#include <linux/of.h>
#include <linux/platform_device.h>
#include <linux/pm_domain.h>
#include <linux/pm_runtime.h>
#include <linux/string.h>

extern const u8 __dtbo_pmp_v1_begin[];
extern const u8 __dtbo_pmp_v1_end[];

static bool hold_at_boot;
core_param(apple_pmp_v1_hold, hold_at_boot, bool, 0444);

static DEFINE_MUTEX(setup_lock);
static bool setup_done;
static struct of_changeset hold_cs;
static struct of_changeset pmp_cs;
static bool held[2];

enum { PS_PMP, PS_PMS_SRAM, PS_COUNT };
static const char *const ps_labels[PS_COUNT] = { "pmp", "pms_sram" };

/* Finds a t8103 PMGR power-state node by its label; takes a reference. */
static struct device_node *find_ps(const char *label)
{
	struct device_node *pmgr, *child;
	const char *l;

	for_each_compatible_node(pmgr, NULL, "apple,t8103-pmgr") {
		for_each_child_of_node(pmgr, child) {
			if (!of_device_is_compatible(child, "apple,t8103-pmgr-pwrstate"))
				continue;
			if (of_property_read_string(child, "label", &l) || strcmp(l, label))
				continue;
			of_node_put(pmgr);
			return child;
		}
	}
	return NULL;
}

static int find_all_ps(struct device_node **ps)
{
	int i;

	for (i = 0; i < PS_COUNT; i++) {
		ps[i] = find_ps(ps_labels[i]);
		if (!ps[i]) {
			pr_err("no PMGR power state \"%s\" in the device tree\n", ps_labels[i]);
			while (i--)
				of_node_put(ps[i]);
			return -ENODEV;
		}
	}
	return 0;
}

static void put_all_ps(struct device_node **ps)
{
	int i;

	for (i = 0; i < PS_COUNT; i++)
		of_node_put(ps[i]);
}

/* Boot-time opt-in: make the PMGR driver register both domains always-on. */
static int __init apple_pmp_v1_hold_init(void)
{
	struct device_node *ps[PS_COUNT];
	int ret = 0, i;

	if (!hold_at_boot)
		return 0;
	if (!of_machine_is_compatible("apple,t8103")) {
		pr_warn("apple_pmp_v1_hold ignored: not a t8103\n");
		return 0;
	}
	if (find_all_ps(ps))
		return 0;

	of_changeset_init(&hold_cs);
	for (i = 0; i < PS_COUNT && !ret; i++) {
		if (!of_property_read_bool(ps[i], "apple,always-on"))
			ret = of_changeset_add_prop_bool(&hold_cs, ps[i], "apple,always-on");
	}
	if (!ret)
		ret = of_changeset_apply(&hold_cs);
	if (ret) {
		of_changeset_destroy(&hold_cs);
		pr_err("could not mark ps_pmp and ps_pms_sram always-on: %d\n", ret);
	} else {
		pr_info("apple_pmp_v1_hold: ps_pmp and ps_pms_sram marked always-on\n");
	}
	put_all_ps(ps);
	return 0;
}
/* Runs before of_platform_default_populate_init() at arch_initcall_sync. */
arch_initcall(apple_pmp_v1_hold_init);

/*
 * Keeps a domain on from now on: a device of ours in it, runtime-active
 * forever, and the domain flagged always-on so system sleep leaves it on too.
 */
static int hold_domain(struct device_node *np, int idx)
{
	struct of_phandle_args spec = { .np = np, .args_count = 0 };
	struct generic_pm_domain *genpd;
	struct platform_device *pdev;
	bool was_on;
	int ret;

	if (held[idx])
		return 0;
	if (of_property_read_bool(np, "apple,always-on")) {
		pr_info("ps_%s: always-on in the device tree\n", ps_labels[idx]);
		held[idx] = true;
		return 0;
	}

	pdev = platform_device_register_data(NULL, "apple-pmp-v1-hold", idx, NULL, 0);
	if (IS_ERR(pdev))
		return PTR_ERR(pdev);

	ret = of_genpd_add_device(&spec, &pdev->dev);
	if (ret) {
		pr_err("ps_%s: cannot join its power domain: %d\n", ps_labels[idx], ret);
		platform_device_unregister(pdev);
		return ret;
	}
	genpd = container_of(pdev->dev.pm_domain, struct generic_pm_domain, domain);
	was_on = genpd->status == GENPD_STATE_ON;

	pm_runtime_enable(&pdev->dev);
	ret = pm_runtime_resume_and_get(&pdev->dev);
	if (ret) {
		pr_err("ps_%s: cannot power it on: %d\n", ps_labels[idx], ret);
		return ret;
	}
	genpd->flags |= GENPD_FLAG_ALWAYS_ON;
	held[idx] = true;

	if (was_on)
		pr_info("ps_%s: held on\n", ps_labels[idx]);
	else
		pr_warn("ps_%s: was off, powered it on and holding it%s\n", ps_labels[idx],
			idx == PS_PMS_SRAM ? " (the firmware image is probably lost: boot with pd_ignore_unused or apple_pmp_v1_hold=1)" : "");
	return 0;
}

static int apply_overlay(struct device_node *ps_pmp)
{
	struct device_node *aic = NULL, *pmp = NULL, *dart = NULL, *mbox = NULL;
	int ovcs_id = 0, ret;

	aic = of_find_compatible_node(NULL, NULL, "apple,aic");
	if (!aic || !aic->phandle || !ps_pmp->phandle) {
		pr_err("no AIC or ps_pmp phandle in the device tree\n");
		ret = -ENODEV;
		goto out;
	}

	ret = of_overlay_fdt_apply(__dtbo_pmp_v1_begin,
				   __dtbo_pmp_v1_end - __dtbo_pmp_v1_begin, &ovcs_id, NULL);
	if (ret) {
		pr_err("overlay failed: %d\n", ret);
		goto out;
	}

	pmp = of_find_compatible_node(NULL, NULL, "apple,t8103-pmp-v1");
	if (pmp) {
		dart = of_parse_phandle(pmp, "iommus", 0);
		mbox = of_parse_phandle(pmp, "mboxes", 0);
	}
	ret = pmp && dart && mbox ? 0 : -ENODEV;

	of_changeset_init(&pmp_cs);
	if (!ret)
		ret = of_changeset_add_prop_u32(&pmp_cs, dart, "interrupt-parent", aic->phandle);
	if (!ret)
		ret = of_changeset_add_prop_u32(&pmp_cs, mbox, "interrupt-parent", aic->phandle);
	if (!ret)
		ret = of_changeset_add_prop_u32(&pmp_cs, dart, "power-domains", ps_pmp->phandle);
	if (!ret)
		ret = of_changeset_add_prop_u32(&pmp_cs, mbox, "power-domains", ps_pmp->phandle);
	if (!ret)
		ret = of_changeset_add_prop_u32(&pmp_cs, pmp, "power-domains", ps_pmp->phandle);
	/* Enabling them creates the platform devices, DART first. */
	if (!ret)
		ret = of_changeset_update_prop_string(&pmp_cs, dart, "status", "okay");
	if (!ret)
		ret = of_changeset_update_prop_string(&pmp_cs, mbox, "status", "okay");
	if (!ret)
		ret = of_changeset_update_prop_string(&pmp_cs, pmp, "status", "okay");
	if (!ret)
		ret = of_changeset_apply(&pmp_cs);
	if (ret) {
		of_changeset_destroy(&pmp_cs);
		of_overlay_remove(&ovcs_id);
		pr_err("enabling the PMP nodes failed: %d\n", ret);
	} else {
		pr_info("added %pOF, %pOF and %pOF\n", pmp, dart, mbox);
	}
out:
	of_node_put(mbox);
	of_node_put(dart);
	of_node_put(pmp);
	of_node_put(aic);
	return ret;
}

/**
 * apple_pmp_v1_dt_setup() - prepare the device tree and power for the PMP
 *
 * Called by the apple-pmp-v1 module before it registers its driver. Safe
 * to call again: later calls do nothing.
 *
 * Return: 0 or a negative errno.
 */
int apple_pmp_v1_dt_setup(void)
{
	struct device_node *ps[PS_COUNT], *pmp;
	int ret, i;

	mutex_lock(&setup_lock);
	if (setup_done) {
		ret = 0;
		goto unlock;
	}
	if (!of_machine_is_compatible("apple,t8103")) {
		pr_err("not a t8103\n");
		ret = -ENODEV;
		goto unlock;
	}
	ret = find_all_ps(ps);
	if (ret)
		goto unlock;

	for (i = 0; i < PS_COUNT && !ret; i++)
		ret = hold_domain(ps[i], i);

	pmp = of_find_compatible_node(NULL, NULL, "apple,t8103-pmp-v1");
	if (!ret && pmp) {
		pr_info("%pOF is already in the device tree, not adding the overlay\n", pmp);
	} else if (!ret) {
		ret = apply_overlay(ps[PS_PMP]);
	}
	of_node_put(pmp);
	put_all_ps(ps);
	if (!ret)
		setup_done = true;
unlock:
	mutex_unlock(&setup_lock);
	return ret;
}
EXPORT_SYMBOL_GPL(apple_pmp_v1_dt_setup);
