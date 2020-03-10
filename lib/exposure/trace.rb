# frozen_string_literal: true

module Exposure
  # Light wrapper around the Core Trace
  class Trace
    def initialize
      @core = Core.new_trace

      @trace_points = []
      @trace_points << TracePoint.new(:class, :call, &method(:push))
      @trace_points << TracePoint.new(:return, :end, &method(:pop))

      @class_name = Class.method(:name).unbind
      @module_name = Module.method(:name).unbind
    end

    def name_of(klass)
      case klass
      when Class then @class_name.bind(klass).call
      when Module then @module_name.bind(klass).call
      end
    end

    def start
      @trace_points.each(&:enable)
    end

    def stop
      @trace_points.each(&:disable)
    end

    def push(trace)
      calla = caller_locations(2..2).first
      receiver = (trace.binding.receiver if trace.binding.receiver.is_a?(Class)) rescue nil
      klass = trace.defined_class

      # First push
      Core.push_frame(
        @core,

        trace.event,

        calla.path,
        calla.lineno,

        trace.path,
        trace.lineno,

        name_of(klass),
        trace.method_id.to_s,

        name_of(receiver)
      )

      # Then add locals
      add_locals(trace.binding)
    end

    def pop(trace)
      if trace.event == :return || trace.event == :b_return
        return_class = trace.return_value.class
        return_type = name_of(return_class)
      end

      add_locals(trace.binding)
      Core.pop_frame(@core, return_type)
    end

    private

    def add_locals(frame_binding)
      frame_binding.local_variables.each do |var|
        begin
          val = frame_binding.local_variable_get(var)
          Core.add_local(@core, var.to_s, name_of(val.class))
        rescue StandardError => e
          Core.add_local(@core, var.to_s, "((#{e.class} during inspect))")
        end
      end
    end
  end
end
