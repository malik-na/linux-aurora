/* SPDX-License-Identifier: MIT */
/* g15c-wordmap [N [STEPS [RUNS]]]: dispatch wordmap.comp over N invocations (default 65536, 256
 * steps, 3 runs) on the first Vulkan device, then report which output words are wrong
 * (analyze.c). Output and meta buffers start at sentinels, so a word the GPU never wrote reads
 * as the sentinel. Exit 0 when every run is right, 1 when any word is wrong, 2 on setup errors.
 * Run under the kit's opt-in: /opt/mesa-m3-g15c/bin/g15c-run ./g15c-wordmap
 *
 * Variants (environment; unset = the t81 configuration):
 *   WM_SHAPE=GX,GY        dispatch a GX x GY grid of groups (N becomes GX*GY*local), linear index
 *   WM_LOCAL=32|64|128    workgroup size (default 64)
 *   WM_VARIANT=nouniform  no descriptors or push constants: addresses, N and steps are
 *                         specialization constants (wordmap_nouniform.spv; needs buffer device address)
 *   WM_SPIN=ITERS         groups WM_SPIN_GROUPS=A-B (linear) spin ITERS rounds first (plain variant)
 *   WM_RAW=PREFIX         save each run's output and meta words to PREFIX-runN-{out,meta}.bin
 * Every run prints its GPU wall time (submit to fence). */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <vulkan/vulkan.h>
#include "analyze.h"

#define CHECK(x) do { VkResult r_ = (x); if (r_ != VK_SUCCESS) { \
   fprintf(stderr, "g15c-wordmap: %s failed: %d\n", #x, r_); exit(2); } } while (0)

static uint32_t *load_spirv(const char *path, size_t *size)
{
   FILE *f = fopen(path, "rb");
   if (!f) { perror(path); exit(2); }
   fseek(f, 0, SEEK_END); *size = ftell(f); fseek(f, 0, SEEK_SET);
   uint32_t *code = malloc(*size);
   if (fread(code, 1, *size, f) != *size) { perror(path); exit(2); }
   fclose(f);
   return code;
}

static void print_report(const struct wm_report *r, int run, int detail)
{
   printf("run %d: wrong %u of %u (sentinel %u, zero %u, other %u; meta unwritten %u), %u runs, %u groups\n",
          run, r->wrong, r->n, r->sentinel, r->zero, r->other, r->meta_unwritten, r->ranges_total, r->groups_wrong);
   if (!detail || !r->wrong)
      return;
   printf("  ranges:");
   for (uint32_t i = 0; i < r->nranges; i++)
      printf(" %u-%u", r->ranges[i].first, r->ranges[i].last);
   if (r->ranges_total > r->nranges)
      printf(" ... (+%u more)", r->ranges_total - r->nranges);
   printf("\n  by group mod 16:");
   for (int i = 0; i < 16; i++)
      printf(" %u", r->group_mod16[i]);
   printf("\n  by lane (0..%u):", (r->group <= 64 ? r->group : 64) - 1);
   for (uint32_t i = 0; i < r->group && i < 64; i++)
      printf(" %u", r->lane[i]);
   printf("\n");
}

static uint32_t env_u32(const char *name, uint32_t dflt)
{
   const char *v = getenv(name);
   return v && *v ? strtoul(v, NULL, 0) : dflt;
}

static void save_raw(const char *prefix, int run, const char *what, const uint32_t *words, uint32_t n)
{
   char path[512];
   snprintf(path, sizeof(path), "%s-run%d-%s.bin", prefix, run, what);
   FILE *f = fopen(path, "wb");
   if (!f || fwrite(words, 4, n, f) != n) { perror(path); exit(2); }
   fclose(f);
}

int main(int argc, char **argv)
{
   uint32_t n = argc > 1 ? strtoul(argv[1], NULL, 0) : 65536;
   uint32_t steps = argc > 2 ? strtoul(argv[2], NULL, 0) : 256;
   int runs = argc > 3 ? atoi(argv[3]) : 3;
   const uint32_t GROUP = env_u32("WM_LOCAL", 64);
   const char *variant = getenv("WM_VARIANT");
   const int nouniform = variant && !strcmp(variant, "nouniform");
   const char *raw = getenv("WM_RAW");
   const uint32_t spin = env_u32("WM_SPIN", 0);
   uint32_t spin_first = 0, spin_last = 0;
   if (getenv("WM_SPIN_GROUPS") && sscanf(getenv("WM_SPIN_GROUPS"), "%u-%u", &spin_first, &spin_last) != 2) {
      fprintf(stderr, "WM_SPIN_GROUPS must be A-B\n"); return 2;
   }
   uint32_t gx, gy = 1;
   if (getenv("WM_SHAPE")) {
      if (sscanf(getenv("WM_SHAPE"), "%u,%u", &gx, &gy) != 2 || !gx || !gy) { fprintf(stderr, "WM_SHAPE must be GX,GY\n"); return 2; }
      n = gx * gy * GROUP;
   } else {
      if (n == 0 || n % GROUP) { fprintf(stderr, "N must be a non-zero multiple of %u\n", GROUP); return 2; }
      gx = n / GROUP;
   }

   VkInstance inst;
   VkApplicationInfo app = { .sType = VK_STRUCTURE_TYPE_APPLICATION_INFO, .apiVersion = VK_API_VERSION_1_2 };
   VkInstanceCreateInfo ici = { .sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO, .pApplicationInfo = &app };
   CHECK(vkCreateInstance(&ici, NULL, &inst));
   uint32_t ndev = 1;
   VkPhysicalDevice pdev;
   VkResult er = vkEnumeratePhysicalDevices(inst, &ndev, &pdev);
   if ((er != VK_SUCCESS && er != VK_INCOMPLETE) || ndev == 0) { fprintf(stderr, "no Vulkan device\n"); return 2; }
   VkPhysicalDeviceProperties props;
   vkGetPhysicalDeviceProperties(pdev, &props);
   printf("device: %s; N %u, steps %u, group %u, grid %ux%u, variant %s%s\n", props.deviceName, n, steps, GROUP,
          gx, gy, nouniform ? "nouniform" : "plain", spin ? ", spin" : "");
   if (spin)
      printf("spin: %u rounds in groups %u-%u\n", spin, spin_first, spin_last);

   uint32_t nq = 16; VkQueueFamilyProperties qf[16];
   vkGetPhysicalDeviceQueueFamilyProperties(pdev, &nq, qf);
   uint32_t family = UINT32_MAX;
   for (uint32_t i = 0; i < nq; i++)
      if (qf[i].queueFlags & VK_QUEUE_COMPUTE_BIT) { family = i; break; }
   if (family == UINT32_MAX) { fprintf(stderr, "no compute queue\n"); return 2; }
   float prio = 1.0f;
   VkDeviceQueueCreateInfo qci = { .sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
      .queueFamilyIndex = family, .queueCount = 1, .pQueuePriorities = &prio };
   VkPhysicalDeviceBufferDeviceAddressFeatures bda = { .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_BUFFER_DEVICE_ADDRESS_FEATURES };
   if (nouniform) {
      VkPhysicalDeviceFeatures2 f2 = { .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2, .pNext = &bda };
      vkGetPhysicalDeviceFeatures2(pdev, &f2);
      if (!bda.bufferDeviceAddress) { fprintf(stderr, "no bufferDeviceAddress\n"); return 2; }
      bda.bufferDeviceAddressCaptureReplay = bda.bufferDeviceAddressMultiDevice = VK_FALSE;
   }
   VkDeviceCreateInfo dci = { .sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO, .queueCreateInfoCount = 1, .pQueueCreateInfos = &qci,
      .pNext = nouniform ? &bda : NULL };
   VkDevice dev;
   CHECK(vkCreateDevice(pdev, &dci, NULL, &dev));
   VkQueue queue;
   vkGetDeviceQueue(dev, family, 0, &queue);

   /* Two host-visible, coherent storage buffers: output and meta. */
   VkPhysicalDeviceMemoryProperties mp;
   vkGetPhysicalDeviceMemoryProperties(pdev, &mp);
   VkBuffer buf[2]; VkDeviceMemory mem[2]; uint32_t *map[2];
   VkDeviceSize size = (VkDeviceSize)n * 4;
   for (int b = 0; b < 2; b++) {
      VkBufferCreateInfo bci = { .sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO, .size = size,
         .usage = VK_BUFFER_USAGE_STORAGE_BUFFER_BIT | (nouniform ? VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT : 0),
         .sharingMode = VK_SHARING_MODE_EXCLUSIVE };
      CHECK(vkCreateBuffer(dev, &bci, NULL, &buf[b]));
      VkMemoryRequirements req;
      vkGetBufferMemoryRequirements(dev, buf[b], &req);
      uint32_t want = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT, type = UINT32_MAX;
      for (uint32_t t = 0; t < mp.memoryTypeCount; t++)
         if ((req.memoryTypeBits & (1u << t)) && (mp.memoryTypes[t].propertyFlags & want) == want) { type = t; break; }
      if (type == UINT32_MAX) { fprintf(stderr, "no host-visible coherent memory\n"); return 2; }
      VkMemoryAllocateFlagsInfo maf = { .sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_FLAGS_INFO, .flags = VK_MEMORY_ALLOCATE_DEVICE_ADDRESS_BIT };
      VkMemoryAllocateInfo mai = { .sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO, .allocationSize = req.size, .memoryTypeIndex = type,
         .pNext = nouniform ? &maf : NULL };
      CHECK(vkAllocateMemory(dev, &mai, NULL, &mem[b]));
      CHECK(vkBindBufferMemory(dev, buf[b], mem[b], 0));
      CHECK(vkMapMemory(dev, mem[b], 0, size, 0, (void **)&map[b]));
   }

   size_t spv_size;
   uint32_t *spv = load_spirv(nouniform ? "wordmap_nouniform.spv" : "wordmap.spv", &spv_size);
   VkShaderModuleCreateInfo smci = { .sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO, .codeSize = spv_size, .pCode = spv };
   VkShaderModule sm;
   CHECK(vkCreateShaderModule(dev, &smci, NULL, &sm));
   VkDescriptorSetLayoutBinding binds[2] = {
      { .binding = 0, .descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, .descriptorCount = 1, .stageFlags = VK_SHADER_STAGE_COMPUTE_BIT },
      { .binding = 1, .descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, .descriptorCount = 1, .stageFlags = VK_SHADER_STAGE_COMPUTE_BIT } };
   VkDescriptorSetLayoutCreateInfo dslci = { .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO, .bindingCount = 2, .pBindings = binds };
   VkDescriptorSetLayout dsl;
   CHECK(vkCreateDescriptorSetLayout(dev, &dslci, NULL, &dsl));
   VkPushConstantRange pcr = { .stageFlags = VK_SHADER_STAGE_COMPUTE_BIT, .size = 8 };
   VkPipelineLayoutCreateInfo plci = { .sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
      .setLayoutCount = nouniform ? 0 : 1, .pSetLayouts = &dsl, .pushConstantRangeCount = nouniform ? 0 : 1, .pPushConstantRanges = &pcr };
   VkPipelineLayout pl;
   CHECK(vkCreatePipelineLayout(dev, &plci, NULL, &pl));
   /* Specialization: constant 0 is the local size in both kernels; the plain kernel takes the
    * spin settings, the no-uniform kernel the buffer addresses, N and the step count. */
   uint32_t spec[7] = { GROUP, spin, spin_first, spin_last, 0, 0, 0 };
   if (nouniform) {
      VkDeviceAddress a[2];
      for (int b = 0; b < 2; b++) {
         VkBufferDeviceAddressInfo bai = { .sType = VK_STRUCTURE_TYPE_BUFFER_DEVICE_ADDRESS_INFO, .buffer = buf[b] };
         a[b] = vkGetBufferDeviceAddress(dev, &bai);
      }
      printf("nouniform: out %#llx meta %#llx\n", (unsigned long long)a[0], (unsigned long long)a[1]);
      uint32_t nu[7] = { GROUP, (uint32_t)a[0], (uint32_t)(a[0] >> 32), (uint32_t)a[1], (uint32_t)(a[1] >> 32), n, steps };
      memcpy(spec, nu, sizeof(spec));
   }
   VkSpecializationMapEntry entries[7];
   for (uint32_t e = 0; e < 7; e++)
      entries[e] = (VkSpecializationMapEntry){ .constantID = e, .offset = e * 4, .size = 4 };
   VkSpecializationInfo si_spec = { .mapEntryCount = nouniform ? 7 : 4, .pMapEntries = entries, .dataSize = sizeof(spec), .pData = spec };
   VkComputePipelineCreateInfo cpci = { .sType = VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO, .layout = pl,
      .stage = { .sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO, .stage = VK_SHADER_STAGE_COMPUTE_BIT, .module = sm,
                 .pName = "main", .pSpecializationInfo = &si_spec } };
   VkPipeline pipe;
   CHECK(vkCreateComputePipelines(dev, VK_NULL_HANDLE, 1, &cpci, NULL, &pipe));

   VkDescriptorPoolSize ps = { VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, 2 };
   VkDescriptorPoolCreateInfo dpci = { .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO, .maxSets = 1, .poolSizeCount = 1, .pPoolSizes = &ps };
   VkDescriptorPool dp;
   CHECK(vkCreateDescriptorPool(dev, &dpci, NULL, &dp));
   VkDescriptorSetAllocateInfo dsai = { .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO, .descriptorPool = dp, .descriptorSetCount = 1, .pSetLayouts = &dsl };
   VkDescriptorSet ds;
   CHECK(vkAllocateDescriptorSets(dev, &dsai, &ds));
   VkDescriptorBufferInfo dbi[2] = { { buf[0], 0, size }, { buf[1], 0, size } };
   VkWriteDescriptorSet w[2];
   for (int b = 0; b < 2; b++)
      w[b] = (VkWriteDescriptorSet){ .sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET, .dstSet = ds, .dstBinding = b,
         .descriptorCount = 1, .descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, .pBufferInfo = &dbi[b] };
   vkUpdateDescriptorSets(dev, 2, w, 0, NULL);

   VkCommandPoolCreateInfo cpi = { .sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO, .queueFamilyIndex = family,
      .flags = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT };
   VkCommandPool cp;
   CHECK(vkCreateCommandPool(dev, &cpi, NULL, &cp));
   VkCommandBufferAllocateInfo cbai = { .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO, .commandPool = cp,
      .level = VK_COMMAND_BUFFER_LEVEL_PRIMARY, .commandBufferCount = 1 };
   VkCommandBuffer cb;
   CHECK(vkAllocateCommandBuffers(dev, &cbai, &cb));
   VkFenceCreateInfo fci = { .sType = VK_STRUCTURE_TYPE_FENCE_CREATE_INFO };
   VkFence fence;
   CHECK(vkCreateFence(dev, &fci, NULL, &fence));

   uint32_t pc[2] = { n, steps };
   int bad = 0;
   for (int run = 1; run <= runs; run++) {
      for (uint32_t i = 0; i < n; i++) { map[0][i] = WM_SENTINEL; map[1][i] = WM_META_SENTINEL; }
      VkCommandBufferBeginInfo bi = { .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO, .flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT };
      CHECK(vkBeginCommandBuffer(cb, &bi));
      vkCmdBindPipeline(cb, VK_PIPELINE_BIND_POINT_COMPUTE, pipe);
      if (!nouniform) {
         vkCmdBindDescriptorSets(cb, VK_PIPELINE_BIND_POINT_COMPUTE, pl, 0, 1, &ds, 0, NULL);
         vkCmdPushConstants(cb, pl, VK_SHADER_STAGE_COMPUTE_BIT, 0, sizeof(pc), pc);
      }
      vkCmdDispatch(cb, gx, gy, 1);
      VkMemoryBarrier mb = { .sType = VK_STRUCTURE_TYPE_MEMORY_BARRIER, .srcAccessMask = VK_ACCESS_SHADER_WRITE_BIT, .dstAccessMask = VK_ACCESS_HOST_READ_BIT };
      vkCmdPipelineBarrier(cb, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, VK_PIPELINE_STAGE_HOST_BIT, 0, 1, &mb, 0, NULL, 0, NULL);
      CHECK(vkEndCommandBuffer(cb));
      VkSubmitInfo si = { .sType = VK_STRUCTURE_TYPE_SUBMIT_INFO, .commandBufferCount = 1, .pCommandBuffers = &cb };
      struct timespec t0, t1;
      clock_gettime(CLOCK_MONOTONIC, &t0);
      CHECK(vkQueueSubmit(queue, 1, &si, fence));
      VkResult wr = vkWaitForFences(dev, 1, &fence, VK_TRUE, 10ull * 1000 * 1000 * 1000);
      clock_gettime(CLOCK_MONOTONIC, &t1);
      double ms = (t1.tv_sec - t0.tv_sec) * 1e3 + (t1.tv_nsec - t0.tv_nsec) / 1e6;
      if (wr != VK_SUCCESS) { printf("run %d: fence wait failed (%d); the job did not complete\n", run, wr); return 1; }
      CHECK(vkResetFences(dev, 1, &fence));
      CHECK(vkResetCommandBuffer(cb, 0));
      struct wm_report r;
      wm_analyze(map[0], map[1], n, GROUP, steps, &r);
      printf("run %d: %.3f ms submit to fence\n", run, ms);
      if (raw) { save_raw(raw, run, "out", map[0], n); save_raw(raw, run, "meta", map[1], n); }
      print_report(&r, run, run == 1 || r.wrong != 0);
      bad |= r.wrong != 0;
   }
   printf("%s\n", bad ? "RED: wrong words" : "GREEN: every word right in every run");
   return bad ? 1 : 0;
}
