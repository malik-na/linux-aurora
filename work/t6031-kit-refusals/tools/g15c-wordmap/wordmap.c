/* SPDX-License-Identifier: MIT */
/* g15c-wordmap [N [STEPS [RUNS]]]: dispatch wordmap.comp over N invocations (default 65536, 256
 * steps, 3 runs) on the first Vulkan device, then report which output words are wrong
 * (analyze.c). Output and meta buffers start at sentinels, so a word the GPU never wrote reads
 * as the sentinel. Exit 0 when every run is right, 1 when any word is wrong, 2 on setup errors.
 * Run under the kit's opt-in: /opt/mesa-m3-g15c/bin/g15c-run ./g15c-wordmap */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <vulkan/vulkan.h>
#include "analyze.h"

#define GROUP 64
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
   printf("\n  by lane (0..63):");
   for (int i = 0; i < GROUP; i++)
      printf(" %u", r->lane[i]);
   printf("\n");
}

int main(int argc, char **argv)
{
   uint32_t n = argc > 1 ? strtoul(argv[1], NULL, 0) : 65536;
   uint32_t steps = argc > 2 ? strtoul(argv[2], NULL, 0) : 256;
   int runs = argc > 3 ? atoi(argv[3]) : 3;
   if (n == 0 || n % GROUP) { fprintf(stderr, "N must be a non-zero multiple of %d\n", GROUP); return 2; }

   VkInstance inst;
   VkApplicationInfo app = { .sType = VK_STRUCTURE_TYPE_APPLICATION_INFO, .apiVersion = VK_API_VERSION_1_1 };
   VkInstanceCreateInfo ici = { .sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO, .pApplicationInfo = &app };
   CHECK(vkCreateInstance(&ici, NULL, &inst));
   uint32_t ndev = 1;
   VkPhysicalDevice pdev;
   VkResult er = vkEnumeratePhysicalDevices(inst, &ndev, &pdev);
   if ((er != VK_SUCCESS && er != VK_INCOMPLETE) || ndev == 0) { fprintf(stderr, "no Vulkan device\n"); return 2; }
   VkPhysicalDeviceProperties props;
   vkGetPhysicalDeviceProperties(pdev, &props);
   printf("device: %s; N %u, steps %u, group %d\n", props.deviceName, n, steps, GROUP);

   uint32_t nq = 16; VkQueueFamilyProperties qf[16];
   vkGetPhysicalDeviceQueueFamilyProperties(pdev, &nq, qf);
   uint32_t family = UINT32_MAX;
   for (uint32_t i = 0; i < nq; i++)
      if (qf[i].queueFlags & VK_QUEUE_COMPUTE_BIT) { family = i; break; }
   if (family == UINT32_MAX) { fprintf(stderr, "no compute queue\n"); return 2; }
   float prio = 1.0f;
   VkDeviceQueueCreateInfo qci = { .sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
      .queueFamilyIndex = family, .queueCount = 1, .pQueuePriorities = &prio };
   VkDeviceCreateInfo dci = { .sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO, .queueCreateInfoCount = 1, .pQueueCreateInfos = &qci };
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
         .usage = VK_BUFFER_USAGE_STORAGE_BUFFER_BIT, .sharingMode = VK_SHARING_MODE_EXCLUSIVE };
      CHECK(vkCreateBuffer(dev, &bci, NULL, &buf[b]));
      VkMemoryRequirements req;
      vkGetBufferMemoryRequirements(dev, buf[b], &req);
      uint32_t want = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT, type = UINT32_MAX;
      for (uint32_t t = 0; t < mp.memoryTypeCount; t++)
         if ((req.memoryTypeBits & (1u << t)) && (mp.memoryTypes[t].propertyFlags & want) == want) { type = t; break; }
      if (type == UINT32_MAX) { fprintf(stderr, "no host-visible coherent memory\n"); return 2; }
      VkMemoryAllocateInfo mai = { .sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO, .allocationSize = req.size, .memoryTypeIndex = type };
      CHECK(vkAllocateMemory(dev, &mai, NULL, &mem[b]));
      CHECK(vkBindBufferMemory(dev, buf[b], mem[b], 0));
      CHECK(vkMapMemory(dev, mem[b], 0, size, 0, (void **)&map[b]));
   }

   size_t spv_size;
   uint32_t *spv = load_spirv("wordmap.spv", &spv_size);
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
      .setLayoutCount = 1, .pSetLayouts = &dsl, .pushConstantRangeCount = 1, .pPushConstantRanges = &pcr };
   VkPipelineLayout pl;
   CHECK(vkCreatePipelineLayout(dev, &plci, NULL, &pl));
   VkComputePipelineCreateInfo cpci = { .sType = VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO, .layout = pl,
      .stage = { .sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO, .stage = VK_SHADER_STAGE_COMPUTE_BIT, .module = sm, .pName = "main" } };
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
      vkCmdBindDescriptorSets(cb, VK_PIPELINE_BIND_POINT_COMPUTE, pl, 0, 1, &ds, 0, NULL);
      vkCmdPushConstants(cb, pl, VK_SHADER_STAGE_COMPUTE_BIT, 0, sizeof(pc), pc);
      vkCmdDispatch(cb, n / GROUP, 1, 1);
      VkMemoryBarrier mb = { .sType = VK_STRUCTURE_TYPE_MEMORY_BARRIER, .srcAccessMask = VK_ACCESS_SHADER_WRITE_BIT, .dstAccessMask = VK_ACCESS_HOST_READ_BIT };
      vkCmdPipelineBarrier(cb, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, VK_PIPELINE_STAGE_HOST_BIT, 0, 1, &mb, 0, NULL, 0, NULL);
      CHECK(vkEndCommandBuffer(cb));
      VkSubmitInfo si = { .sType = VK_STRUCTURE_TYPE_SUBMIT_INFO, .commandBufferCount = 1, .pCommandBuffers = &cb };
      CHECK(vkQueueSubmit(queue, 1, &si, fence));
      VkResult wr = vkWaitForFences(dev, 1, &fence, VK_TRUE, 10ull * 1000 * 1000 * 1000);
      if (wr != VK_SUCCESS) { printf("run %d: fence wait failed (%d); the job did not complete\n", run, wr); return 1; }
      CHECK(vkResetFences(dev, 1, &fence));
      CHECK(vkResetCommandBuffer(cb, 0));
      struct wm_report r;
      wm_analyze(map[0], map[1], n, GROUP, steps, &r);
      print_report(&r, run, run == 1 || r.wrong != 0);
      bad |= r.wrong != 0;
   }
   printf("%s\n", bad ? "RED: wrong words" : "GREEN: every word right in every run");
   return bad ? 1 : 0;
}
