/* SPDX-License-Identifier: GPL-2.0-only OR MIT */
/*
 * Apple SoC PMGR device power state driver: interface for consumers
 */

#ifndef _LINUX_SOC_APPLE_PMGR_PWRSTATE_H_
#define _LINUX_SOC_APPLE_PMGR_PWRSTATE_H_

#include <linux/errno.h>
#include <linux/types.h>

struct device;

#if IS_REACHABLE(CONFIG_APPLE_PMGR_PWRSTATE)
int apple_pmgr_ps_sleep_release(struct device *consumer, bool release);
int apple_pmgr_ps_check_on(struct device *consumer, const char **off);
#else
static inline int apple_pmgr_ps_sleep_release(struct device *consumer,
					      bool release)
{
	return -EOPNOTSUPP;
}

static inline int apple_pmgr_ps_check_on(struct device *consumer,
					 const char **off)
{
	*off = NULL;
	return -EOPNOTSUPP;
}
#endif

#endif /* _LINUX_SOC_APPLE_PMGR_PWRSTATE_H_ */
